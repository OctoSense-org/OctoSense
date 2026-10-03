"""Tests for tools/release-scan.py (no network; containers built in a temp dir)."""
import contextlib
import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release_scan", ROOT / "tools/release-scan.py")
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)


def findings(data, extra=()):
    out = []
    scan.scan_bytes(data, "x", scan.compile_patterns(extra, identity=False), out)
    return out


class PatternTests(unittest.TestCase):
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
        def appimage(path, runtime=b"", filesystem=b"", after=b"", file=b"app", link=b"usr/bin/app"):
            # A shell script standing in for the runtime: it answers the two
            # options the scan uses. The filesystem starts at 4096 with a
            # squashfs superblock whose bytes_used (offset 40) covers it.
            script = (b"#!/bin/sh\n# " + runtime + b"\ncase \"$1\" in\n"
                      b"  --appimage-offset) echo 4096 ;;\n"
                      b"  --appimage-extract) mkdir -p squashfs-root/usr/bin && printf '%s' '" + file
                      + b"' > squashfs-root/usr/bin/app && ln -s '" + link + b"' squashfs-root/AppRun ;;\n"
                      b"esac\nexit 0\n")
            squashfs = b"hsqs" + bytes(36) + (48 + len(filesystem)).to_bytes(8, "little") + filesystem
            path.write_bytes(script.ljust(4096, b"\n") + squashfs + after)

        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "octosense_0.1.0_x86_64.AppImage"
            # zstd's literals back to back: "/home/" then the next run's "io/".
            appimage(path, filesystem=b"\x28\xb5\x2f\xfd/home/io/\x91\x07")
            code, out = self.run_scan(path)
            self.assertEqual(code, 0, out)
            for kind, where in ((dict(runtime=b"/Users/someone/runtime"), "AppImage (runtime)"),
                                (dict(after=b"/home/someone/x"), "AppImage (after its filesystem)"),
                                (dict(file=b"/Users/someone/src"), "AppImage/usr/bin/app"),
                                (dict(link=b"/home/someone/app"), "AppImage/AppRun (link target)")):
                appimage(path, **kind)
                code, out = self.run_scan(path)
                self.assertEqual(code, 1, kind)
                self.assertIn(where, out)
            path.write_bytes(b"#!/bin/sh\necho 0\n")
            code, out = self.run_scan(path)
            self.assertEqual(code, 1)
            self.assertIn("no squashfs filesystem", out)

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
