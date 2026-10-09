"""Tests for desktop/scripts/package.py (no build, no network)."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import plistlib
import shutil
import struct
import tarfile
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location("package", HERE / "package.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
appimage = package.appimage_tool()


def synthetic_kernel(patched=False, text=b'code', rodata=b'constants'):
    """Non-executable ELF fixture: real section/dynamic/symbol table layouts."""
    strings = b'\0libone.so\0libtwo.so\0'
    names = ['', '.text', '.rodata', '.dynstr', '.dynamic', '.symtab', '.strtab', '.shstrtab']
    labels = b'\0' + b'\0'.join(name.encode() for name in names[1:]) + b'\0'
    if patched:
        names[1], names[2] = names[2], names[1]  # patchelf may reorder section indices
    dynaddr = 0x7000 if patched else 0x6000
    dynamic = [(1, 1), (1, 11), (5, dynaddr), (10, len(strings) + (len(appimage.RUNPATH) if patched else 0))]
    if patched:
        dynamic.append((29, len(strings)))
    dynamic.append((0, 0))
    contents = {'': b'', '.text': text, '.rodata': rodata,
                '.dynstr': strings + (appimage.RUNPATH if patched else b''),
                '.dynamic': b''.join(struct.pack('<qQ', *entry) for entry in dynamic),
                '.symtab': (struct.pack('<IBBHQQ', 1, 2, 0, names.index('.text'), 0x4000, len(text)) +
                            struct.pack('<IBBHQQ', 6, 0, 2, names.index('.dynamic'), 0x8000, 0)),
                '.strtab': b'\0main\0_DYNAMIC\0', '.shstrtab': labels}
    data, records = bytearray(64), []
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHIQ', data, 16, 3, 62, 1, 0x4000)
    for name in names:
        kind = {'.dynamic': 6, '.symtab': 2, '.strtab': 3, '.shstrtab': 3, '.dynstr': 3}.get(name, 1)
        address = {'.text': 0x4000, '.rodata': 0x5000, '.dynstr': dynaddr,
                   '.dynamic': 0x9000 if patched else 0x8000}.get(name, 0)
        records.append((labels.index(name.encode() + b'\0') if name else 0, kind, 0, address,
                        len(data), len(contents[name]), names.index('.strtab') if name == '.symtab' else 0,
                        0, 8 if patched and name == '.dynstr' else 1, 24 if name == '.symtab' else 0))
        data.extend(contents[name])
    struct.pack_into('<Q', data, 40, len(data))
    struct.pack_into('<HHHHHH', data, 52, 64, 0, 0, 64, len(names), names.index('.shstrtab'))
    data.extend(b''.join(struct.pack('<IIQQQQIIQQ', *record) for record in records))
    return bytes(data)


def synthetic_deb(path, kernel, receipt):
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w:gz') as output:
        for name, data in ((appimage.KERNEL, kernel), (appimage.RECEIPT, json.dumps(receipt).encode())):
            info = tarfile.TarInfo('./' + name)
            info.size = len(data)
            output.addfile(info, io.BytesIO(data))
    payload = archive.getvalue()
    header = f"{'data.tar.gz/':<16}{0:<12}{0:<6}{0:<6}{'100644':<8}{len(payload):<10}`\n".encode()
    path.write_bytes(b'!<arch>\n' + header + payload + (b'\n' if len(payload) % 2 else b''))


class AppImageReceiptTests(unittest.TestCase):
    def setUp(self):
        self.raw = synthetic_kernel()
        self.patched = synthetic_kernel(patched=True)
        self.staged = {'revision': 'a' * 40, 'version': 'octos test', 'source': 'prebuilt',
                       'sha256': appimage.sha(self.raw)}

    def test_stale_appimage_receipt_is_corrected_without_changing_raw_metadata(self):
        original = dict(self.staged)
        corrected = appimage.updated_receipt(self.staged, self.staged, self.raw, self.patched)
        self.assertEqual(corrected, {**original, 'sha256': appimage.sha(self.patched)})
        self.assertNotEqual(corrected['sha256'], original['sha256'])
        self.assertEqual(self.staged, original)
        self.assertEqual(appimage.updated_receipt(corrected, self.staged, self.raw, self.patched), corrected)

    def test_foreign_receipts_or_changed_code_are_not_blessed(self):
        for key in ('revision', 'version', 'sha256'):
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                appimage.updated_receipt({**self.staged, key: 'wrong'}, self.staged, self.raw, self.patched)
        for blob in (synthetic_kernel(True, text=b'changed'), synthetic_kernel(True, rodata=b'changed')):
            with self.assertRaisesRegex(RuntimeError, 'section contents'):
                appimage.updated_receipt(self.staged, self.staged, self.raw, blob)
        for offset in (24, 48):
            changed = bytearray(self.patched)
            changed[offset] ^= 1
            with self.assertRaisesRegex(RuntimeError, 'ELF identity'):
                appimage.updated_receipt(self.staged, self.staged, self.raw, bytes(changed))
        changed = self.patched.replace(appimage.RUNPATH, b'$ORIGIN/evil!\0')
        with self.assertRaises(RuntimeError):
            appimage.updated_receipt(self.staged, self.staged, self.raw, changed)

    def test_only_exact_unchanged_hidden_dynamic_anchor_is_admitted(self):
        anchor = struct.pack('<IBBHQQ', 6, 0, 2, 4, 0x8000, 0)
        self.assertIn(anchor, self.patched)
        for replacement in (struct.pack('<IBBHQQ', 6, 0, 2, 4, 0x8001, 0),
                            struct.pack('<IBBHQQ', 6, 1, 2, 4, 0x8000, 0),
                            struct.pack('<IBBHQQ', 6, 0, 0, 4, 0x8000, 0),
                            struct.pack('<IBBHQQ', 6, 0, 2, 4, 0x8000, 1)):
            with self.assertRaisesRegex(RuntimeError, 'symbol identities'):
                appimage.verify_runpath_transform(self.raw, self.patched.replace(anchor, replacement))

    def test_only_aligned_dynamic_string_relocation_can_change_alignment(self):
        # The positive fixture has the real patchelf .dynstr alignment 1→8.
        appimage.verify_runpath_transform(self.raw, self.patched)
        _, ordered = appimage.elf_sections(self.patched)
        table = int.from_bytes(self.patched[40:48], 'little')
        for name, field_offset, value in (('.text', 48, 8), ('.dynstr', 48, 16),
                                          ('.dynstr', 16, 0x7001)):
            changed = bytearray(self.patched)
            struct.pack_into('<Q', changed, table + ordered.index(name) * 64 + field_offset, value)
            with self.assertRaisesRegex(RuntimeError, 'section alignment'):
                appimage.verify_runpath_transform(self.raw, bytes(changed))

    def test_format_bindings_keep_deb_and_appimage_kernel_hashes_distinct(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            deb, image = temp / 'current.deb', temp / 'current.AppImage'
            synthetic_deb(deb, self.raw, self.staged)
            image.write_bytes(b'packaged AppImage')
            binding = {'file': image.name, 'format': 'appimage', 'sha256': appimage.sha(image.read_bytes()),
                       'kernel': {**self.staged, 'sha256': appimage.sha(self.patched)}}
            original_deb = deb.read_bytes()
            with mock.patch.object(appimage, 'finalize_appimage', return_value=binding):
                results = appimage.finalize_linux(temp, ['deb', 'appimage'], self.staged, self.raw)
            self.assertEqual(results[0]['kernel'], self.staged)
            self.assertEqual(results[1]['kernel'], binding['kernel'])
            self.assertEqual(deb.read_bytes(), original_deb)
            synthetic_deb(deb, self.patched, self.staged)
            with self.assertRaisesRegex(RuntimeError, 'DEB kernel'):
                appimage.deb_binding(deb, self.staged)

    def test_owner_and_hardlink_checks_do_not_silently_discard_metadata(self):
        appimage.root_owners(b'-rwxr-xr-x 0/0 12 2026-01-01 00:00 root/file\n')
        for listing in (b'', b'-rwxr-xr-x 1000/0 12 2026-01-01 00:00 root/file\n'):
            with self.assertRaises(RuntimeError):
                appimage.root_owners(listing)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'one').write_bytes(b'shared')
            os.link(root / 'one', root / 'two')
            self.assertEqual(appimage.inventory(root)['one']['hardlinks'], ['one', 'two'])

    @unittest.skipUnless(shutil.which('mksquashfs') and shutil.which('unsquashfs'), 'requires Linux squashfs-tools')
    def test_real_squashfs_repack_changes_only_receipt_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            tree = temp / 'input'
            for relative, content in ((appimage.KERNEL, self.patched),
                                      (appimage.RECEIPT, json.dumps(self.staged).encode()),
                                      ('usr/lib/fixture', b'unchanged resource')):
                path = tree / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            (tree / appimage.KERNEL).chmod(0o755)
            os.link(tree / 'usr/lib/fixture', tree / 'usr/lib/hardlink')
            (tree / 'usr/lib/symlink').symlink_to('fixture')
            fs = temp / 'input.squashfs'
            appimage.run([shutil.which('mksquashfs'), str(tree), str(fs), '-noappend', '-no-progress',
                          '-processors', '2', '-all-root', '-no-xattrs', '-comp', 'gzip', '-mkfs-time', '0'])
            runtime = bytearray(synthetic_kernel())
            runtime[8:11] = b'AI\x02'
            image = temp / 'test.AppImage'
            image.write_bytes(runtime + fs.read_bytes())
            image.chmod(0o755)
            original = image.read_bytes()
            binding = appimage.finalize_appimage(image, self.staged, self.raw)
            self.assertNotEqual(image.read_bytes(), original)
            self.assertEqual(image.read_bytes()[:len(runtime)], runtime)
            self.assertEqual(binding['kernel']['sha256'], appimage.sha(self.patched))
            sealed = image.read_bytes()
            repeated = appimage.finalize_appimage(image, self.staged, self.raw)
            self.assertEqual(repeated['kernel'], binding['kernel'])
            self.assertEqual(repeated['sha256'], binding['sha256'])
            self.assertFalse(repeated['finalization']['receipt_updated'])
            self.assertEqual(image.read_bytes(), sealed)
            invalid = {**self.staged, 'sha256': '0' * 64}
            with self.assertRaises(RuntimeError):
                appimage.finalize_appimage(image, invalid, self.raw)
            self.assertEqual(image.read_bytes(), sealed, 'failed repair preserves its input')


class VersionTests(unittest.TestCase):
    def test_the_version_comes_from_a_desktop_tag(self):
        self.assertEqual(package.version_from_tag("desktop-v0.2.0"), "0.2.0")
        self.assertEqual(package.version_from_tag("refs/tags/desktop-v1.0.0-rc.1"), "1.0.0-rc.1")
        for tag in ("home-v0.2.0", "rom-v20260919-j", "desktop-0.2.0", "desktop-v0.2", "desktop-vX"):
            with self.assertRaises(ValueError, msg=tag):
                package.version_from_tag(tag)

    def test_the_default_version_is_the_packages(self):
        self.assertRegex(package.cargo_version(), r"^\d+\.\d+\.\d+")


class EnvironmentTests(unittest.TestCase):
    def test_each_os_reads_resources_where_its_package_puts_them(self):
        root = Path("/build/OctoSense")
        mac = package.package_env("macos", root, {"HOME": "/home/me"})
        self.assertEqual((mac["MAKEPAD"], mac["MAKEPAD_PACKAGE_DIR"]), ("apple_bundle", "."))
        self.assertEqual(mac["MACOSX_DEPLOYMENT_TARGET"], "11.0")
        win = package.package_env("windows", root, {"USERPROFILE": r"C:\Users\me"})
        self.assertEqual(win["MAKEPAD_PACKAGE_DIR"], ".")
        self.assertNotIn("MAKEPAD", win)
        linux = package.package_env("linux", root, {"HOME": "/home/me"})
        # usr/bin/octosense -> usr/lib/octosense, in the .deb and the AppImage.
        self.assertEqual(linux["MAKEPAD_PACKAGE_DIR"], "../lib/octosense")
        for env in (mac, win, linux):
            self.assertEqual(env["CARGO_PROFILE_RELEASE_STRIP"], "debuginfo")
            self.assertTrue(Path(env["MAKEPAD_APP_ICON_ICO"]).is_file())
            self.assertTrue(Path(env["MAKEPAD_APP_ICON_1024"]).is_file())

    def test_paths_are_remapped_with_the_checkout_last(self):
        env = package.package_env("linux", Path("/home/me/src/OctoSense"),
                                  {"HOME": "/home/me", "RUSTFLAGS": "-C target-cpu=native"})
        flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f")
        self.assertEqual(flags[:2], ["-C", "target-cpu=native"], "existing flags are kept")
        remaps = [flags[i + 1] for i, f in enumerate(flags) if f == "--remap-path-prefix"]
        self.assertEqual(remaps, ["/home/me=~", "/home/me/.cargo=/cargo", "/home/me/src/OctoSense=."])
        custom = package.package_env("macos", Path("/b/OctoSense"), {"HOME": "/h", "CARGO_HOME": "/c h/cargo"})
        self.assertIn("/c h/cargo=/cargo", custom["CARGO_ENCODED_RUSTFLAGS"].split("\x1f"), "a space survives")


class ResourceTests(unittest.TestCase):
    def test_linked_git_and_path_crates_with_resources_are_staged(self):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)

            def crate(name, source=None, resources=True, version="1.0.0"):
                directory = temp / name
                (directory / "resources/icons").mkdir(parents=True) if resources else directory.mkdir()
                if resources:
                    (directory / "resources/icons/a.svg").write_text("<svg/>")
                    (directory / "resources/README.md").write_text("provenance")
                return {"name": name, "version": version, "source": source,
                        "manifest_path": str(directory / "Cargo.toml")}

            metadata = {"packages": [
                crate("makepad-widgets"),
                crate("rinx", "git+https://github.com/hagency-org/Rinx.git?rev=abc#abc"),
                crate("some-registry-crate", "registry+https://github.com/rust-lang/crates.io-index"),
                crate("octosense-appcard"),  # in the workspace, not linked by this build
                crate("no-resources", resources=False),
            ]}
            linked = {("makepad-widgets", "1.0.0"), ("rinx", "1.0.0"), ("some-registry-crate", "1.0.0"),
                      ("no-resources", "1.0.0")}
            crates = package.resource_crates(metadata, linked)
            self.assertEqual([name for name, _ in crates], ["makepad_widgets", "rinx"])
            staged = package.stage_resources(crates, temp / "out/resources")
            self.assertTrue((staged / "makepad_widgets/resources/icons/a.svg").is_file(),
                            "the layout Makepad looks up: <crate_name>/resources/<file>")
            self.assertFalse((staged / "rinx/resources/README.md").exists(), "docs stay out of the package")
            package.stage_resources(crates[:1], staged)
            self.assertFalse((staged / "rinx").exists(), "a restage starts clean")


class ConfigTests(unittest.TestCase):
    def test_packaged_macos_calendar_access_has_usage_descriptions_and_entitlement(self):
        packaging = HERE.parent / "packaging"
        config = json.loads((packaging / "release.json").read_text())
        with (packaging / config["macos"]["infoPlistPath"]).open("rb") as stream:
            info = plistlib.load(stream)
        # EventKit uses the legacy key before macOS 14 and the full-access
        # key on newer systems. The host refuses requests when either
        # applicable packaged declaration is missing.
        for key in ("NSCalendarsUsageDescription", "NSCalendarsFullAccessUsageDescription"):
            with self.subTest(key=key):
                self.assertIsInstance(info.get(key), str)
                self.assertTrue(info[key].strip())
        with (packaging / config["macos"]["entitlements"]).open("rb") as stream:
            entitlements = plistlib.load(stream)
        self.assertIs(entitlements.get("com.apple.security.personal-information.calendars"), True)

    def test_the_release_config_is_filled_in_per_build(self):
        base = json.loads((HERE.parent / "packaging/release.json").read_text())
        self.assertEqual(base["identifier"], "org.octosense.desktop")
        self.assertEqual(base["productName"], "OctoSense")
        self.assertIn("libwebkit2gtk-4.1-0 | libwebkit2gtk-4.0-37", base["deb"]["depends"])
        self.assertIn("libgtk-3-0t64 | libgtk-3-0", base["deb"]["depends"])
        for icon in base["icons"]:
            self.assertTrue((HERE.parent / "packaging" / icon).is_file(), icon)
        config = package.packager_config(base, version="0.2.0", binaries_dir=Path("/t/release"), out_dir=Path("/t/dist"),
                                         resources=Path("/t/res"), kernel=Path("/t/bin/octos-kernel"))
        self.assertEqual(config["version"], "0.2.0")
        self.assertEqual(config["resources"], [{"src": "/t/res", "target": "."}])
        self.assertEqual(config["externalBinaries"], ["/t/bin/octos-kernel"])
        self.assertNotIn("signingIdentity", config["macos"], "the build is always unsigned")
        self.assertNotIn("windows", config)
        self.assertNotIn("version", base, "the base config is not modified")
        none = package.packager_config(base, version="0.2.0", binaries_dir=Path("/t"), out_dir=Path("/t"),
                                       resources=Path("/t"), kernel=None)
        self.assertNotIn("externalBinaries", none, "--no-kernel ships none")

    def test_the_build_never_sees_a_signing_variable(self):
        base = {"PATH": "/bin", "RUSTFLAGS": "-C x", "APPLE_SIGNING_IDENTITY": "Developer ID Application: X (T)",
                "APPLE_CERTIFICATE": "secret", "APPLE_API_KEY_PATH": "/k.p8", "APPLE_API_KEY_P8": "synthetic-p8",
                "WINDOWS_CERTIFICATE_THUMBPRINT": "AB12"}
        env = package.build_env({"MAKEPAD_PACKAGE_DIR": "."}, base)
        self.assertEqual(env["PATH"], "/bin")
        self.assertEqual(env["MAKEPAD_PACKAGE_DIR"], ".")
        self.assertNotIn("RUSTFLAGS", env, "folded into CARGO_ENCODED_RUSTFLAGS")
        self.assertNotIn("APPLE_API_KEY_P8", env, "the inline signing credential must not reach a build")
        for name in package.SIGNING_ENV:
            self.assertNotIn(name, env)
        self.assertIn("APPLE_SIGNING_IDENTITY", base, "the caller's environment is not modified")

    @unittest.skipIf(package.host_os() == "windows", "the stand-in kernel is a POSIX shell script")
    def test_the_kernel_ships_with_its_receipt_where_the_desktop_looks(self):
        tool = package.kernel_tool()
        revision = tool.octos_revision()
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            kernel = temp / "octos"
            kernel.write_text(f"#!/bin/sh\necho 'octos 2.0.3 ({revision[:7]} 2026-10-01)'\n")
            kernel.chmod(0o755)
            resources = temp / "resources"
            resources.mkdir()
            sidecar, receipt = package.stage_kernel(kernel, "prebuilt", temp / "out", resources)
            self.assertEqual(receipt["revision"], revision)
            self.assertEqual(json.loads((resources / package.RECEIPT_NAME).read_text()), receipt,
                             "the receipt travels with the resources (Contents/Resources, usr/lib/octosense)")
            shipped = sidecar.with_name(f"{package.KERNEL_NAME}-{package.host_triple()}")
            self.assertTrue(shipped.is_file(), "cargo-packager's externalBinaries name")
            self.assertEqual(receipt["sha256"], hashlib.sha256(shipped.read_bytes()).hexdigest())
            wrong = temp / "wrong"
            wrong.write_text("#!/bin/sh\necho 'octos 2.0.3 (0000000 2026-10-01)'\n")
            wrong.chmod(0o755)
            with self.assertRaises(RuntimeError):
                package.stage_kernel(wrong, "prebuilt", temp / "out2", resources)

    def test_the_kernel_is_built_by_kernel_artifact(self):
        steps, binary, source = package.kernel_tool().kernel_plan(host=True, work=Path("/w"))
        self.assertTrue(source.startswith("https://github.com/octos-org/octos.git@"))
        self.assertIn("--target-dir", steps[-1][1])
        self.assertFalse((ROOT / "tools/build-desktop.py").exists(), "one kernel tool, not two")

    def test_the_sidecar_name_is_the_one_the_kernel_service_looks_for(self):
        launch = (HERE.parents[1] / "crates/kernel/src/launch.rs").read_text()
        self.assertIn(f'"{package.KERNEL_NAME}"', launch)
        self.assertIn(f'"{package.RECEIPT_NAME}"', launch)


if __name__ == "__main__":
    unittest.main()
