"""Tests for tools/release-scan.py (no network; containers built in a temp dir)."""
import contextlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release_scan", ROOT / "tools/release-scan.py")
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)


def findings(data, extra=()):
    out = []
    scan.scan_bytes(data, "x", scan.compile_patterns(extra, identity=False), out)
    return out


def appimage_bytes(runtime=b"", filesystem=b"", after=b"", elf_class=2, order="little", section_last=False):
    """Synthetic type-2 ELF; extraction is mocked, never an executable fixture."""
    data = bytearray(4096)
    data[:11] = b"\x7fELF" + bytes((elf_class, 1 if order == "little" else 2, 1, 0)) + b"AI\x02"
    header, shoff_pos, word, ehsize_pos, entsize_pos, count_pos, size, section_pos = (
        (52, 32, 4, 40, 46, 48, 40, 16) if elf_class == 1 else (64, 40, 8, 52, 58, 60, 64, 24))

    def put(pos, width, value):
        data[pos:pos + width] = value.to_bytes(width, order)

    table = 2048 if section_last else 4096 - size
    for pos, width, value in ((shoff_pos, word, table), (ehsize_pos, 2, header),
                              (entsize_pos, 2, size), (count_pos, 2, 1)):
        put(pos, width, value)
    if section_last:
        put(table + section_pos, word, 3072)
        put(table + section_pos + word, word, 1024)
    data[header:header + len(runtime)] = runtime
    superblock = bytearray(96)
    superblock[:4] = b"hsqs"
    superblock[28:32] = b"\x04\x00\x00\x00"
    superblock[40:48] = (96 + len(filesystem)).to_bytes(8, "little")
    return bytes(data + superblock + filesystem + after)


class PatternTests(unittest.TestCase):
    def test_only_proven_octoscode_placeholder_seams_are_ignored(self):
        themes = b"/home/user/src/octoscode-app/home/user/src/octosSystemSolarizedSlateClaudeCodexLight"
        folder = b"/home/user/codeUse this folderb1_br_use"
        pooled_folder = b"b1_br_path/home/user/codeb1_br_use"
        for known in (themes, folder, pooled_folder):
            self.assertEqual(findings(known), [])
            for leak in (known + b"\x00/home/user/private", b"/home/user/private\x00" + known,
                         known.replace(b"/home/user/", b"/home/someone/", 1),
                         known[:-1] + b"!"):
                self.assertTrue(findings(leak), leak)
            self.assertTrue(findings(known, extra=[r"/home/user/"]))
        for standalone in (b"/home/user/src/octoscode-app", b"/home/user/src/octos", b"/home/user/code"):
            self.assertTrue(findings(standalone), standalone)
        for tampered in (
            pooled_folder.replace(b"b1_br_path", b"another_field"),
            pooled_folder.replace(b"b1_br_use", b"another_button"),
            pooled_folder.replace(b"/home/user/code", b"/home/user/private"),
        ):
            self.assertTrue(findings(tampered), tampered)

    def test_exact_public_design_assets_are_not_private_build_data(self):
        fixture = ROOT / "tools/fixtures/release-scan"
        for name in ("workspace.card", "browser.json", "pairing.json"):
            data = (fixture / name).read_bytes()
            self.assertEqual(findings(data), [], name)
            self.assertEqual(findings(b"binary prefix\x00" + data + b"\x00binary suffix"), [], name)

    def test_public_asset_exceptions_are_whole_asset_and_match_scoped(self):
        fixture = ROOT / "tools/fixtures/release-scan"
        for name in ("workspace.card", "browser.json", "pairing.json"):
            data = (fixture / name).read_bytes()
            matches = [m.group(0) for label, regex in scan.compile_patterns(identity=False)
                       for m in regex.finditer(data)
                       if label in ("Linux home directory", "private IPv4 address")]
            self.assertTrue(matches, name)
            for literal in matches:
                # An identical private-looking value outside the verified asset
                # remains a finding, regardless of which occurrence comes first.
                for combined in (literal + b"private\x00" + data,
                                 data + b"\x00" + literal + b"private"):
                    self.assertTrue(findings(combined), name)
                self.assertTrue(findings(literal + b"private"), name)
                self.assertTrue(findings(literal), "a standalone identical literal must fail: " + name)
            self.assertTrue(findings(b"!" + data[1:]), "one altered byte must invalidate " + name)
            self.assertTrue(findings(data[:-1]), "truncation must invalidate " + name)
            self.assertTrue(findings(data, extra=[scan.re.escape(matches[0].decode())]), name)
            identity = [("the scanning account's name", scan.re.compile(scan.re.escape(matches[0])))]
            out = []
            scan.scan_bytes(data, "public", identity, out)
            self.assertTrue(out, "public assets must never suppress identity checks")

    def test_missing_or_malformed_public_asset_metadata_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "release-scan.py"
            metadata = script.with_name("release-scan-public-assets.json")
            with mock.patch.object(scan, "__file__", str(script)):
                try:
                    scan.public_asset_matches.cache_clear()
                    with self.assertRaises(FileNotFoundError):
                        findings(b"/home/user/private")
                    for invalid in ("{", '{"assets":[{"size":1,"sha256":"invalid","matches":[]}]}'):
                        metadata.write_text(invalid)
                        scan.public_asset_matches.cache_clear()
                        with self.assertRaises(ValueError):
                            findings(b"/home/user/private")
                finally:
                    scan.public_asset_matches.cache_clear()

    def test_private_paths_and_hosts_are_found(self):
        for leak in (b"/Users/someone/src/app.rs", b"C:\\Users\\someone\\.cargo", b"c:/Users/someone/x",
                     "C:\\Users\\".encode("utf-16-le"), b"/home/someone/.cargo/registry", b"built on studio.local", b"my-mac.local", b"http://studio.local/api", 
                     b"http://192.168.1.20:8080", b"10.0.0.7", b"172.20.1.1"):
            self.assertTrue(findings(b"at " + leak + b" end"), leak)

    def test_ci_and_system_paths_are_fine(self):
        for fine in (b"/home/runner/work/OctoSense", b"C:\\Users\\runneradmin\\.cargo\\registry", b"/usr/local/lib", b"/rustc/abc/library/std", b"~/.cargo/registry",
                     b"./crates/shell/src/lib.rs", b"localhost.localdomain", b"127.0.0.1", b"1.10.0.7",
                     b"version 10.2.3.4.5", b"/cargo/registry/src", b"EHLO octosense.local\r\n",
                     b"fleet-worker@octos.local", b"e2e@test.local",
                     b"forbiddenutf-8.local/bin/ominix-api", b"x.local/share/y",
                     b"not found.forbiddenutf-8.local/bin/x", b"command not foundnewTab.local/sharekde-open"):
            self.assertEqual(findings(fine), [], fine)

    def test_a_dependency_constant_is_not_a_host(self):
        # matrix-sdk's "send-queue.localhost": x86-64 code keeps its first 16
        # bytes as a comparison constant, and any constant may come next.
        for fine in (b"send-queue.local\x00\x00\x00\x00", b"send-queue.local\x80\x10Hk",
                     b"mxc://send-queue.localhost/txn"):
            self.assertEqual(findings(fine), [], fine)
        # That exact name only: a host whose name merely contains it is found.
        for leak in (b"my-send-queue.local", b"send-queue2.local", b"xsend-queue.local"):
            self.assertTrue(findings(b"at " + leak + b" end"), leak)

    def test_only_the_proven_public_rinx_source_seam_is_ignored(self):
        root = b"/cargo/git/checkouts/rinx-cf0dcd4e7b4d3fa8/4b89097/src"
        first = root + b"/home/main_desktop_ui.rs"
        second = root + b"/home/tombstone_footer.rs"
        self.assertEqual(findings(first + second), [])
        for leak in (
            b"/home/main_desktop_ui.rs/private",
            b"/home/someone/private",
            first + b"/private",
            first + second.replace(b"rinx-", b"other-"),
            first + second.replace(b"4b89097", b"1234567"),
            first + second.replace(b"tombstone_footer", b"unproven_file"),
            (first + second).replace(b"/cargo/", b"/private/"),
            first + second + b"\x00/home/main_desktop_ui.rs/private",
        ):
            self.assertTrue(findings(leak), leak)

    def test_only_the_proven_mail_literal_seam_is_ignored(self):
        prefix = b"Mail service is not "
        apparent_host = b"registeredattemptssendoctosense.local"
        self.assertEqual(findings(prefix + apparent_host + b"\x00"), [])
        for leak in (
            apparent_host,
            b"https://" + apparent_host + b"/",
            b"Other service is not " + apparent_host,
            prefix + b"registeredattemptssendprivate.local",
            prefix + b"registeredattemptsotheroctosense.local",
            prefix + b"registeredattemptssendoctosense2.local",
            prefix + apparent_host + b"\x00" + apparent_host,
        ):
            self.assertTrue(findings(leak), leak)

    def test_findings_are_masked_and_extra_patterns_apply(self):
        out = findings(b"/Users/someone/x")
        self.assertEqual(len(out), 1)
        self.assertNotIn("someone", out[0], "the match itself is not echoed")
        self.assertTrue(findings(b"the build host is zeta-box", extra=["zeta-box"]))
        self.assertEqual(findings(b"clean", extra=["", " "]), [], "empty patterns are ignored")

    def test_identity_patterns_skip_generic_accounts(self):
        self.assertEqual(scan.identity_patterns("runner", "fv-az123"), [])
        self.assertEqual(scan.identity_patterns("root", "localhost"), [])
        labels = [label for label, _ in scan.identity_patterns("jdoe-dev", "workbench")]
        self.assertEqual(len(labels), 2)


class ContainerTests(unittest.TestCase):
    def run_scan(self, *paths):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = scan.main(["--no-identity", *map(str, paths)])
        return code, out.getvalue()

    def test_leaks_inside_packages_fail_the_scan(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            payload = b"binary /Users/someone/.cargo/registry/src"
            # A directory (an .app), a zip, a tar.gz and a .deb (ar + data.tar.gz).
            app = temp / "OctoSense.app/Contents/MacOS"
            app.mkdir(parents=True)
            (app / "octosense").write_bytes(payload)
            with zipfile.ZipFile(temp / "a.zip", "w", zipfile.ZIP_DEFLATED) as z:
                z.writestr("OctoSense.app/Contents/MacOS/octosense", payload)
            tar_bytes = io.BytesIO()
            with tarfile.open(fileobj=tar_bytes, mode="w:gz") as t:
                info = tarfile.TarInfo("./usr/bin/octosense")
                info.size = len(payload)
                t.addfile(info, io.BytesIO(payload))
            (temp / "a.tar.gz").write_bytes(tar_bytes.getvalue())
            blob = tar_bytes.getvalue()
            deb = b"!<arch>\n"
            for name, data in (("debian-binary", b"2.0\n"), ("data.tar.gz", blob)):
                deb += f"{name + '/':<16}{0:<12}{0:<6}{0:<6}{'100644':<8}{len(data):<10}`\n".encode() + data
                if len(data) % 2:
                    deb += b"\n"
            (temp / "octosense_0.1.0_amd64.deb").write_bytes(deb)
            for artifact in ("OctoSense.app", "a.zip", "a.tar.gz", "octosense_0.1.0_amd64.deb"):
                code, out = self.run_scan(temp / artifact)
                self.assertEqual(code, 1, artifact)
                self.assertIn("macOS user directory", out)

    def test_a_link_target_is_read(self):
        with tempfile.TemporaryDirectory() as temp:
            app = Path(temp) / "OctoSense.app"
            app.mkdir()
            (app / "Applications").symlink_to("/Applications")
            (app / "icon.png").symlink_to("usr/share/icons/octosense.png")
            code, out = self.run_scan(app)
            self.assertEqual(code, 0, out)
            (app / "leak").symlink_to("/home/someone/build/icon.png")
            code, out = self.run_scan(app)
            self.assertEqual(code, 1)
            self.assertIn("OctoSense.app/leak (link target): Linux home directory", out)

    def test_an_appimage_is_read_except_its_compressed_filesystem(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "octosense_0.1.0_x86_64.AppImage"
            def extract(command, **kwargs):
                self.assertEqual(command[0], "/trusted/unsquashfs")
                self.assertEqual(command[command.index("-offset") + 1], "4096")
                self.assertIn("-strict-errors", command)
                self.assertTrue(kwargs["check"])
                self.assertNotIn("GH_TOKEN", kwargs["env"])
                self.assertNotIn("RELEASE_SCAN_EXTRA", kwargs["env"])
                self.assertEqual(Path(command[-1]).read_bytes(), path.read_bytes())
                self.assertEqual(Path(command[-1]).stat().st_mode & 0o111, 0)
                root = Path(command[command.index("-dest") + 1])
                (root / "usr/bin").mkdir(parents=True)
                (root / "usr/bin/app").write_bytes(file)
                (root / "AppRun").symlink_to(link.decode())

            with mock.patch.object(scan.shutil, "which", return_value="/trusted/unsquashfs"), \
                    mock.patch.object(scan.subprocess, "run", side_effect=extract):
                file, link = b"app", b"usr/bin/app"
                # Compressed literal runs are not logical file content.
                path.write_bytes(appimage_bytes(filesystem=b"\x28\xb5\x2f\xfd/home/io/\x91\x07"))
                code, out = self.run_scan(path)
                self.assertEqual(code, 0, out)
                for kind, where in ((dict(runtime=b"/Users/someone/runtime"), "AppImage (runtime)"),
                                    (dict(after=b"/home/someone/x"), "AppImage (after its filesystem)"),
                                    (dict(file=b"/Users/someone/src"), "AppImage/usr/bin/app"),
                                    (dict(link=b"/home/someone/app"), "AppImage/AppRun (link target)")):
                    file, link = kind.get("file", b"app"), kind.get("link", b"usr/bin/app")
                    path.write_bytes(appimage_bytes(**{k: v for k, v in kind.items() if k not in ("file", "link")}))
                    code, out = self.run_scan(path)
                    self.assertEqual(code, 1, kind)
                    self.assertIn(where, out)

    def test_appimage_never_executes_an_artifact_to_find_its_offset(self):
        with tempfile.TemporaryDirectory() as temp:
            path, marker = Path(temp) / "malicious.AppImage", Path(temp) / "exfiltrated"
            # The old scanner chmod'ed and ran this with inherited credentials.
            path.write_text('#!/bin/sh\nprintf "%s" "$GH_TOKEN" > "$ATTACK_MARKER"\necho 4096\n')
            path.chmod(0o755)
            with mock.patch.dict(scan.os.environ, {"GH_TOKEN": "synthetic-token", "ATTACK_MARKER": str(marker)}), \
                    mock.patch.object(scan.subprocess, "run", wraps=scan.subprocess.run) as run:
                code, out = self.run_scan(path)
            self.assertEqual(code, 1, out)
            self.assertIn("not a type-2 ELF AppImage", out)
            run.assert_not_called()
            self.assertFalse(marker.exists(), "no executable artifact may inherit the publication token")

    def test_appimage_elf_classes_byte_orders_and_section_layouts(self):
        for elf_class in (1, 2):
            for order in ("little", "big"):
                for section_last in (False, True):
                    with self.subTest(elf_class=elf_class, order=order, section_last=section_last):
                        self.assertEqual(scan.appimage_filesystem(appimage_bytes(
                            elf_class=elf_class, order=order, section_last=section_last)), (4096, 4192))

    def test_appimage_malformed_headers_fail_before_any_extractor(self):
        good = appimage_bytes()
        cases = [good[:15], good[:60], good[:4090], good[:4191]]
        for pos, data in ((8, b"AI\x01"), (4, b"\x03"), (5, b"\x00"), (6, b"\x00"),
                          (52, bytes(2)), (58, bytes(2)), (60, bytes(2)), (40, (2**64-1).to_bytes(8, "little")),
                          (4096, b"nope"), (4096 + 28, bytes(4)),
                          (4096 + 40, (95).to_bytes(8, "little")),
                          (4096 + 40, (2**64-1).to_bytes(8, "little"))):
            cases.append(good[:pos] + data + good[pos + len(data):])
        with tempfile.TemporaryDirectory() as temp, mock.patch.object(scan.subprocess, "run") as run:
            path = Path(temp) / "bad.AppImage"
            for data in cases:
                path.write_bytes(data)
                code, out = self.run_scan(path)
                self.assertEqual(code, 1, out)
            run.assert_not_called()

    def test_appimage_missing_failed_or_empty_extractor_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "test.AppImage"
            path.write_bytes(appimage_bytes())
            with mock.patch.object(scan.shutil, "which", return_value=None):
                code, out = self.run_scan(path)
                self.assertEqual(code, 1, out)
                self.assertIn("unsquashfs missing", out)
            with mock.patch.object(scan.shutil, "which", return_value="/trusted/unsquashfs"):
                with mock.patch.object(scan.subprocess, "run", side_effect=scan.subprocess.CalledProcessError(1, "unsquashfs")):
                    self.assertEqual(self.run_scan(path)[0], 1)
                with mock.patch.object(scan.subprocess, "run"):
                    code, out = self.run_scan(path)
                    self.assertEqual(code, 1, out)
                    self.assertIn("did not produce a filesystem directory", out)

    def test_a_clean_artifact_passes(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "SHA256SUMS"
            path.write_text("abc  OctoSense_0.1.0_aarch64.dmg\n")
            code, out = self.run_scan(path)
            self.assertEqual(code, 0)
            self.assertIn("clean", out)

    def test_a_dmg_is_opened_unless_told_its_contents_were_scanned(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "OctoSense_0.1.0_aarch64.dmg"
            path.write_bytes(b"not really a disk image")
            code, _ = self.run_scan("--dmg-bytes-only", path)
            self.assertEqual(code, 0)
            if scan.sys.platform != "darwin":
                code, out = self.run_scan(path)
                self.assertEqual(code, 1, "off macOS a .dmg cannot be opened, so it fails")
            leaky = Path(temp) / "leaky.dmg"
            leaky.write_bytes(b"x /Users/someone/y")
            code, out = self.run_scan("--dmg-bytes-only", leaky)
            self.assertEqual(code, 1, "its bytes are still scanned")

    def test_an_unopenable_container_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "broken.deb"
            path.write_bytes(b"not an ar archive")
            code, out = self.run_scan(path)
            self.assertEqual(code, 1)
            self.assertIn("could not be opened", out)


if __name__ == "__main__":
    unittest.main()
