"""Tests for desktop/scripts/package.py (no build, no network)."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("package", HERE / "package.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


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
    def test_the_release_config_is_filled_in_per_build(self):
        base = json.loads((HERE.parent / "packaging/release.json").read_text())
        self.assertEqual(base["identifier"], "org.octosense.desktop")
        self.assertEqual(base["productName"], "OctoSense")
        for icon in base["icons"]:
            self.assertTrue((HERE.parent / "packaging" / icon).is_file(), icon)
        config = package.packager_config(base, version="0.2.0", binaries_dir=Path("/t/release"), out_dir=Path("/t/dist"),
                                         resources=Path("/t/res"), kernel=Path("/t/bin/octos-kernel"), env={})
        self.assertEqual(config["version"], "0.2.0")
        self.assertEqual(config["resources"], [{"src": "/t/res", "target": "."}])
        self.assertEqual(config["externalBinaries"], ["/t/bin/octos-kernel"])
        self.assertNotIn("signingIdentity", config["macos"], "unsigned without a signing identity")
        self.assertNotIn("windows", config)
        self.assertNotIn("version", base, "the base config is not modified")
        signed = package.packager_config(base, version="0.2.0", binaries_dir=Path("/t"), out_dir=Path("/t"),
                                         resources=Path("/t"), kernel=None,
                                         env={"APPLE_SIGNING_IDENTITY": "Developer ID Application: X (T)",
                                              "WINDOWS_CERTIFICATE_THUMBPRINT": "AB12"})
        self.assertEqual(signed["macos"]["signingIdentity"], "Developer ID Application: X (T)")
        self.assertEqual(signed["windows"]["certificateThumbprint"], "AB12")
        self.assertTrue(signed["windows"]["tsp"])
        self.assertNotIn("externalBinaries", signed, "--no-kernel ships none")

    def test_the_sidecar_name_is_the_one_the_kernel_service_looks_for(self):
        launch = (HERE.parents[1] / "crates/kernel/src/launch.rs").read_text()
        self.assertIn(f'"{package.KERNEL_NAME}"', launch)


if __name__ == "__main__":
    unittest.main()
