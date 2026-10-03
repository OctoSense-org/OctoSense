import configparser
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


build = load("build_home", "scripts/build-home.py")
stage = load("stage_home", "scripts/stage-home.py")


class BuildTests(unittest.TestCase):
    def args(self, *extra):
        return build.arguments(["--sdk", "/sdk with spaces", "--android-sdk", "/android", *extra])

    def test_standalone_requires_an_explicit_signer_choice(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "standalone")

    def test_rom_rejects_development_signing(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "rom", "--development")

    def test_rom_requires_both_signing_inputs(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "rom", "--sign-key", "/keys/platform.pk8")

    def test_outputs_are_separate_and_builds_do_not_deploy(self):
        ordinary = self.args("--variant", "standalone", "--development", "--offline")
        rom = self.args("--variant", "rom", "--sign-key", "/keys/platform.pk8", "--sign-cert", "/keys/platform.x509.pem")
        self.assertNotEqual(ordinary.output, rom.output)
        steps = build.build_plan(ordinary)
        commands = [command for _, command in steps]
        self.assertEqual(steps[-1][0], ROOT.parent / "phone")
        self.assertIn("--sdk-path=/sdk with spaces", commands[-1])
        self.assertIn("--no-sign", commands[-1])
        self.assertTrue(all("--offline" in c for c in commands[1:] if c[0] != "git"))
        self.assertFalse(any(c[:2] == ["git", "fetch"] for c in commands), "offline builds fetch nothing")
        self.assertFalse(any("adb" in c or "fastboot" in c for c in commands))
        self.assertFalse(any("OctoSense-mobile" in arg for c in commands for arg in c))

    def test_developer_options_are_separate_from_signing_and_keep_release_optimization(self):
        for variant, signer in (("standalone", ["--development"]),
                                ("rom", ["--sign-key", "/keys/platform.pk8", "--sign-cert", "/keys/platform.x509.pem"])):
            with self.subTest(variant=variant):
                ordinary = self.args("--variant", variant, *signer)
                developer = self.args("--variant", variant, *signer, "--dev-mode")
                normal_plan, dev_plan = build.build_plan(ordinary), build.build_plan(developer)
                self.assertNotIn("dev-mode", normal_plan[-1][1])
                self.assertEqual(dev_plan[:-1], normal_plan[:-1], "kernel, Bridge and packager are unchanged")
                self.assertEqual(dev_plan[-1][1], normal_plan[-1][1] + ["--features", "dev-mode"])
                self.assertIn("--release", dev_plan[-1][1])
                self.assertNotEqual(ordinary.output, developer.output)

    def test_test_package_only_changes_home_and_is_recorded_in_dry_run(self):
        package = "dev.makepad.octosense.studio"
        argv = ["--variant", "standalone", "--development", "--dev-mode", "--package-name", package,
                "--sdk", "/sdk with spaces", "--android-sdk", "/android", "--no-octos-kernel", "--dry-run"]
        with contextlib.redirect_stdout(io.StringIO()) as output:
            build.main(argv)
        plan = json.loads(output.getvalue())
        self.assertTrue(plan["development"])
        self.assertTrue(plan["dev_mode"])
        self.assertEqual(plan["home_package"], package)
        self.assertFalse(plan["installs_or_flashes"])
        self.assertTrue(plan["output"].endswith("standalone-dev-mode/" + package))
        commands = [step["argv"] for step in plan["steps"]]
        self.assertIn("--package-name=" + package, commands[-1])
        self.assertLess(commands[-1].index("--package-name=" + package), commands[-1].index("build"))
        self.assertFalse(any("--package-name=" + package in c for c in commands[:-1]))

    def test_bad_package_names_and_rom_overrides_are_refused(self):
        for package in ("bare", "dev.test/escape", "dev.test..app", "dev.test.$value", "dev.1app"):
            with self.subTest(package=package), contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                self.args("--variant", "standalone", "--development", "--package-name", package)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "rom", "--sign-key", "/keys/platform.pk8", "--sign-cert", "/keys/platform.x509.pem",
                      "--package-name", "dev.makepad.octosense.studio")

    def test_receipt_requires_the_requested_apk_identity(self):
        badging = "package: name='dev.makepad.octosense.studio' versionCode='42' versionName='1'\n"
        self.assertEqual(build.home_version(badging, "dev.makepad.octosense.studio"), 42)
        for text in (badging, "no package in this APK"):
            with self.assertRaises(RuntimeError):
                build.home_version(text, "dev.makepad.octosense")

    def test_existing_packager_skips_tool_compilation(self):
        args = self.args("--variant", "standalone", "--development", "--packager", "/tools/cargo-makepad", "--no-octos-kernel")
        plan = build.build_plan(args)
        self.assertEqual(len(plan), 3)
        self.assertEqual(plan[-1][1][0], "/tools/cargo-makepad")

    def test_the_default_phone_build_bundles_the_pinned_octos_kernel(self):
        args = self.args("--variant", "standalone", "--development")
        steps, kernel = build.kernel_plan(args)
        revision = build.octos_revision()
        commands = [command for _, command in steps]
        self.assertIn(["git", "fetch", "--quiet", "--no-tags", "--depth=1", build.OCTOS_URL, revision], commands)
        self.assertIn(["git", "checkout", "--quiet", "--detach", revision], commands)
        cargo = commands[-1]
        self.assertEqual(cargo[0], "env")
        self.assertTrue(any(arg.startswith("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=/sdk with spaces/ndk/") for arg in cargo))
        self.assertEqual(cargo[cargo.index("cargo"):], ["cargo", "build", "--locked", "--release", "--target",
                                                        "aarch64-linux-android", *build.OCTOS_KERNEL_BUILD])
        self.assertEqual(kernel, build.REPO / ".sources/octos-kernel/target/aarch64-linux-android/release/octos")
        self.assertIn(f"CARGO_TARGET_DIR={build.REPO / '.sources/octos-kernel/target'}", cargo)
        self.assertEqual(build.extra_libs(kernel), f"liboctos.so={kernel}")
        # The kernel is built before the APK that bundles it.
        plan = [command for _, command in build.build_plan(args)]
        self.assertLess(plan.index(cargo), len(plan) - 1)

    def test_a_prebuilt_kernel_or_none(self):
        steps, kernel = build.kernel_plan(self.args("--variant", "standalone", "--development", "--octos-kernel", "/k/octos"))
        self.assertEqual((steps, kernel), ([], Path("/k/octos")))
        steps, kernel = build.kernel_plan(self.args("--variant", "standalone", "--development", "--no-octos-kernel"))
        self.assertEqual((steps, kernel), ([], None))
        self.assertIsNone(build.extra_libs(None))
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            self.args("--variant", "standalone", "--development", "--octos-kernel", "/k/octos", "--no-octos-kernel")

    def test_the_octos_revision_is_the_one_cargo_lock_pins(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "Cargo.lock"
            rev = "6ad76e5c1e659bdf10ec05ae869428b48edccf7f"
            lock.write_text(f'[[package]]\nname = "octos-cli"\nversion = "2.0.3"\nsource = "git+https://github.com/octos-org/octos.git?rev={rev}#{rev}"\n')
            self.assertEqual(build.octos_revision(lock), rev)
            lock.write_text("[[package]]\nname = \"serde\"\n")
            with self.assertRaises(RuntimeError):
                build.octos_revision(lock)


    def test_rustflags_remap_the_checkout_and_cargo_home(self):
        flags = build.rustflags({"RUSTFLAGS": "-C debuginfo=0", "CARGO_HOME": "/opt/cargo"})
        self.assertTrue(flags.startswith("-C debuginfo=0 "))
        self.assertIn("--remap-path-prefix=/opt/cargo=/cargo", flags)
        # rustc applies the last matching remap: the checkout wins over home.
        self.assertTrue(flags.endswith(f"--remap-path-prefix={build.REPO}=/octosense"))

    def test_personal_paths_scan_only_native_libraries(self):
        import zipfile
        users = b"/Users" + b"/"  # split so the tracked-path guard does not flag the fixture
        with tempfile.TemporaryDirectory() as temp:
            apk = Path(temp) / "home.apk"
            with zipfile.ZipFile(apk, "w") as archive:
                archive.writestr("lib/arm64-v8a/libclean.so", b"/cargo/registry/src/x.rs\0/octosense/phone")
                archive.writestr("lib/arm64-v8a/libleak.so", b"\0" + users + b"Shared/build/cargo/git/x.rs\0")
                archive.writestr("assets/readme.txt", users + b"someone")
            self.assertEqual(build.personal_paths(apk, home="/nonexistent-home"),
                             {"lib/arm64-v8a/libleak.so": [users.decode()]})


class StagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.receipt = {"schema_version": 1, "variant": "rom", "development": False, "artifacts": {}}
        for name in ("OctoSenseHome.apk", "OctoSenseBridge.apk"):
            data = name.encode()
            (self.directory / name).write_bytes(data)
            self.receipt["artifacts"][name] = {"sha256": hashlib.sha256(data).hexdigest(), "certificate_sha256": "abc"}

    def write_receipt(self):
        (self.directory / "build.json").write_text(json.dumps(self.receipt))

    def test_accepts_the_recorded_pair(self):
        self.write_receipt()
        self.assertEqual(stage.verify(self.directory), self.receipt)

    def test_rejects_standalone_on_the_rom_channel(self):
        self.receipt["variant"] = "standalone"
        self.write_receipt()
        with self.assertRaises(ValueError):
            stage.verify(self.directory)

    def test_rom_channel_rejects_developer_options_even_with_platform_signing(self):
        self.receipt["dev_mode"] = True
        self.write_receipt()
        with self.assertRaisesRegex(ValueError, "developer mode"):
            stage.verify(self.directory)

    def test_rom_channel_rejects_a_home_test_identity(self):
        self.receipt["home_package"] = "dev.makepad.octosense.studio"
        self.write_receipt()
        with self.assertRaisesRegex(ValueError, "test package"):
            stage.verify(self.directory)

    def test_rejects_an_apk_replaced_after_signing(self):
        self.write_receipt()
        (self.directory / "OctoSenseHome.apk").write_bytes(b"another APK")
        with self.assertRaises(ValueError):
            stage.verify(self.directory)

    def test_rejects_different_home_bridge_signers(self):
        self.receipt["artifacts"]["OctoSenseBridge.apk"]["certificate_sha256"] = "def"
        self.write_receipt()
        with self.assertRaises(ValueError):
            stage.verify(self.directory)


class NativeLibraryStagingTests(unittest.TestCase):
    """Home is staged without its lib/ entries: its libraries (the octos
    kernel among them) are installed as files in the app's lib/arm64 dir."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.apk = self.directory / "built.apk"
        self.out = self.directory / "prebuilt"
        self.out.mkdir()

    def write_apk(self, libraries):
        with zipfile.ZipFile(self.apk, "w") as archive:
            archive.writestr("AndroidManifest.xml", b"manifest")
            archive.writestr(zipfile.ZipInfo("resources.arsc"), b"table")
            archive.writestr("classes.dex", b"dex", compress_type=zipfile.ZIP_DEFLATED)
            for name, data in libraries.items():
                archive.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)

    def test_libraries_leave_the_apk_for_the_lib_dir(self):
        self.write_apk({"lib/arm64-v8a/libmakepad.so": b"home", "lib/arm64-v8a/liboctos.so": b"kernel"})
        stale = self.out / "lib/arm64/libold.so"
        stale.parent.mkdir(parents=True)
        stale.write_bytes(b"old")
        self.assertEqual(stage.split_native_libraries(self.apk, self.out), ["libmakepad.so", "liboctos.so"])
        self.assertEqual((self.out / "lib/arm64/libmakepad.so").read_bytes(), b"home")
        self.assertEqual((self.out / "lib/arm64/liboctos.so").read_bytes(), b"kernel")
        self.assertFalse(stale.exists())
        with zipfile.ZipFile(self.out / "OctoSenseHome.apk") as staged:
            self.assertEqual(staged.namelist(), ["AndroidManifest.xml", "resources.arsc", "classes.dex"])
            self.assertEqual(staged.getinfo("resources.arsc").compress_type, zipfile.ZIP_STORED)
            self.assertEqual(staged.read("classes.dex"), b"dex")

    def test_a_home_without_its_kernel_is_refused(self):
        self.write_apk({"lib/arm64-v8a/libmakepad.so": b"home"})
        with self.assertRaises(ValueError):
            stage.split_native_libraries(self.apk, self.out)
        self.assertFalse((self.out / "OctoSenseHome.apk").exists())

    def test_other_abis_are_refused(self):
        self.write_apk({"lib/arm64-v8a/libmakepad.so": b"home", "lib/arm64-v8a/liboctos.so": b"kernel",
                        "lib/x86_64/libmakepad.so": b"x86"})
        with self.assertRaises(ValueError):
            stage.split_native_libraries(self.apk, self.out)


class VendorLayerTests(unittest.TestCase):
    """The product layer installs what stage-home.py stages, and the kernel
    can run under SELinux enforcing."""

    vendor = ROOT / "vendor/octosense"

    def test_android_mk_installs_each_staged_library_into_homes_lib_dir(self):
        mk = (self.vendor / "Android.mk").read_text()
        self.assertIn("LOCAL_MODULE := OctoSenseHome\n", mk)
        for line in ("LOCAL_MODULE_CLASS := APPS", "LOCAL_SRC_FILES := prebuilt/OctoSenseHome.apk",
                     "LOCAL_CERTIFICATE := platform", "LOCAL_PRIVILEGED_MODULE := true",
                     "LOCAL_SYSTEM_EXT_MODULE := true", "include $(BUILD_PREBUILT)"):
            self.assertIn(line + "\n", mk)
        libraries = re.search(r"LOCAL_PREBUILT_JNI_LIBS := \\\n((?:\s+\S+(?: \\)?\n)+)", mk).group(1).split()
        self.assertEqual([lib for lib in libraries if lib != "\\"],
                         [f"prebuilt/{stage.STAGED_LIB_DIR}/{name}" for name in stage.HOME_LIBRARIES])
        # One definition only: Android.bp keeps the other imports.
        self.assertNotIn('name: "OctoSenseHome"', (self.vendor / "Android.bp").read_text())

    def test_the_kernel_is_executable_on_the_image(self):
        config = configparser.ConfigParser()
        config.read(self.vendor / "config.fs")
        entry = config["system_ext/priv-app/OctoSenseHome/lib/arm64/liboctos.so"]
        self.assertEqual(entry["mode"], "0755")
        self.assertIn("TARGET_FS_CONFIG_GEN += vendor/octosense/config.fs",
                      (self.vendor / "BoardConfigOctoSense.mk").read_text())

    def test_the_kernel_may_serve_its_goal_socket(self):
        policy = (self.vendor / "sepolicy/private/octosense_kernel.te").read_text()
        self.assertIn("allow platform_app app_data_file:sock_file { create getattr setattr unlink write };", policy)
        # Nothing else is granted: the other kernel denials are benign probes.
        rules = [line for line in policy.splitlines() if line and not line.startswith("#")]
        self.assertEqual([rule for rule in rules if rule.startswith("allow")],
                         ["allow platform_app app_data_file:sock_file { create getattr setattr unlink write };"])
        self.assertTrue(all(rule.startswith(("allow ", "dontaudit ")) for rule in rules))


if __name__ == "__main__":
    unittest.main()
