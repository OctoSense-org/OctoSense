import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_apps", ROOT / "tools/native_apps.py")
native_apps = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native_apps)

GENERATED = ["Cargo.toml", "crates/shell/Cargo.toml", "desktop/Cargo.toml", "phone/Cargo.toml",
             native_apps.RUST_FILE, native_apps.MANIFEST]


def quiet():
    return contextlib.redirect_stderr(io.StringIO())


class TheRepository(unittest.TestCase):
    def test_every_generated_place_matches_the_manifest(self):
        with quiet(), contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(native_apps.main(["--check"], root=ROOT), 0, out.getvalue())

    def test_the_manifest_declares_todays_native_apps(self):
        apps = native_apps.load(ROOT)
        self.assertEqual([app["id"] for app in apps], ["rinx", "reference", "sheets", "terminal", "appcard", "apphub"])
        hosting = {app["id"]: app["hosting"] for app in apps}
        # Terminal is the only process app for now (ADR 0004 §2).
        self.assertEqual(hosting["terminal"]["macos"], "process")
        self.assertEqual(hosting["terminal"]["windows"], "process")
        self.assertEqual(hosting["terminal"]["linux"], "process-if-vulkan")
        for ident in ("apphub", "rinx", "sheets", "reference", "appcard"):
            self.assertEqual(set(hosting[ident].values()), {"module"}, ident)
        # Non-Vulkan Linux is in-process for everything.
        for ident, h in hosting.items():
            self.assertIn(h["linux"], ("module", "process-if-vulkan"), ident)


class Fixture(unittest.TestCase):
    """A copy of the files the generator reads and writes."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for rel in GENERATED:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / rel, self.root / rel)
        self.data = json.loads((self.root / native_apps.MANIFEST).read_text())

    def app(self, ident):
        return next(app for app in self.data["apps"] if app["id"] == ident)

    def save(self):
        (self.root / native_apps.MANIFEST).write_text(json.dumps(self.data, indent=2) + "\n")

    def run_main(self, *argv):
        with quiet(), contextlib.redirect_stdout(io.StringIO()):
            return native_apps.main(list(argv), root=self.root)

    def assertRefused(self, pattern):
        with self.assertRaisesRegex(native_apps.ManifestError, pattern):
            native_apps.validate(self.data)


class Validation(Fixture):
    def test_refuses_process_on_mobile_and_wasm(self):
        for target in ("android", "ios", "ohos", "wasm"):
            self.app("terminal")["hosting"][target] = "process"
            self.assertRefused(rf"hosting\.{target}: {target} has no processes")
            self.app("terminal")["hosting"][target] = "module"

    def test_refuses_plain_process_on_linux(self):
        self.app("terminal")["hosting"]["linux"] = "process"
        self.assertRefused(r"terminal: hosting\.linux: 'process' would run without Vulkan\+Wayland")

    def test_refuses_process_without_a_bin(self):
        self.app("rinx")["hosting"]["macos"] = "process"
        self.assertRefused(r"rinx: hosting\.macos: 'process' needs a bin")
        self.app("rinx")["hosting"]["macos"] = "process-if-vulkan"
        self.assertRefused(r"needs a bin")

    def test_refuses_unknown_values_and_missing_targets(self):
        self.app("sheets")["hosting"]["macos"] = "thread"
        del self.app("sheets")["hosting"]["ios"]
        self.app("sheets")["shells"]["phone"] = "sometimes"
        self.assertRefused(r"(?s)hosting\.ios is missing.*'thread' is not one of.*shells\.phone")

    def test_refuses_duplicates_and_short_revisions(self):
        self.data["apps"].append(copy.deepcopy(self.app("terminal")))
        self.app("rinx")["source"]["rev"] = "68afcf79"
        self.assertRefused(r"(?s)source\.rev must be a full commit id.*duplicate id")

    def test_tool_policy_is_checked(self):
        self.app("terminal")["agent"]["tool_policy"] = {"run": {"confirm": "maybe", "auto_approvable": False}}
        self.assertRefused(r"tool_policy\.run\.confirm must be 'host' or 'app'")
        self.app("terminal")["agent"]["tool_policy"] = {"run": {"confirm": "host"}}
        self.assertRefused(r"tool_policy\.run: needs exactly confirm and auto_approvable")

    def test_sandbox_and_storage_are_checked(self):
        self.app("terminal")["sandbox"] = {"network": "some", "processes": "yes"}
        self.assertRefused(r"sandbox\.network must be one of none, any")
        self.app("terminal")["sandbox"] = {"network": "any", "processes": True, "gpu": True}
        self.assertRefused(r"sandbox needs exactly network and processes")
        self.app("terminal")["sandbox"] = {"network": "any", "processes": True}
        self.app("terminal")["storage"]["external"] = ["home:rw", "/etc:ro"]
        self.assertRefused(r"storage\.external: '/etc:ro'")
        self.app("terminal")["storage"]["external"] = ["home/../x:rw"]
        self.assertRefused(r"storage\.external")

    def test_the_whole_storage_block_is_checked_as_the_shell_parses_it(self):
        storage = self.app("rinx")["storage"]
        storage["quota"] = 1
        self.assertRefused(r"storage\.quota is not a storage field")
        del storage["quota"]
        storage["agent_workspace"] = "home"
        self.assertRefused(r"storage\.agent_workspace must be 'account' or 'none'")
        storage["agent_workspace"] = "account"
        for bad in (0, -1, True, "1G", 1.5):
            storage["max_bytes"] = bad
            self.assertRefused(r"storage\.max_bytes must be a positive whole number")
        storage["max_bytes"] = 1 << 29
        storage["cache_max_bytes"] = 0
        self.assertRefused(r"storage\.cache_max_bytes must be a positive whole number")

    def test_the_storage_block_reaches_the_shell_whole(self):
        self.app("rinx")["storage"]["max_bytes"] = 536870912
        self.save()
        self.assertEqual(self.run_main("--no-lock"), 0)
        rust = (self.root / native_apps.RUST_FILE).read_text()
        self.assertIn('storage: r#"{"accounts": true, "agent_workspace": "account", "external": [], "max_bytes": 536870912}"#', rust)

    def test_the_terminals_commands_are_host_confirmed_and_never_auto_approved(self):
        self.assertEqual(self.app("terminal")["agent"]["tool_policy"],
                         {"run": {"confirm": "host", "auto_approvable": False}})
        rust = native_apps.render_rust(native_apps.validate(self.data))
        self.assertIn('ToolPolicy { tool: "run", confirm: Confirm::Host, auto_approvable: false }', rust)

    def test_refuses_unknown_keys(self):
        self.app("reference")["hosted"] = "yes"
        self.assertRefused(r"reference: unknown hosted")


class Generation(Fixture):
    def test_a_manifest_change_is_drift_until_regenerated(self):
        self.app("terminal")["hosting"]["macos"] = "module"
        self.save()
        self.assertEqual(self.run_main("--check"), 1)
        self.assertEqual(self.run_main("--no-lock"), 0)
        self.assertEqual(self.run_main("--check"), 0)
        rust = (self.root / native_apps.RUST_FILE).read_text()
        self.assertIn('bin: Some("terminal"),\n        macos: Hosting::Module,', rust)

    def test_a_hand_edit_inside_a_block_is_drift(self):
        path = self.root / "crates/shell/Cargo.toml"
        path.write_text(path.read_text().replace('app-terminal = ["dep:makepad-terminal"]', 'app-terminal = []'))
        self.assertEqual(self.run_main("--check"), 1)

    def test_a_new_app_reaches_every_place(self):
        extra = copy.deepcopy(self.app("sheets"))
        extra.update({"id": "notes", "crate": "makepad-notes", "module": "makepad_notes::NOTES_MODULE", "bin": "notes",
                      "shells": {"desktop": "default", "phone": "off"}, "native_mobile": "feature"})
        extra["source"]["local"] = ".sources/makepad/apps/notes"
        self.data["apps"].append(extra)
        self.save()
        self.assertEqual(self.run_main("--no-lock"), 0)
        root = (self.root / "Cargo.toml").read_text()
        rev = extra["source"]["rev"]  # the Makepad pin, whatever it is today
        self.assertIn(f'makepad-notes = {{ git = "https://github.com/OctoSense-org/makepad.git", rev = "{rev}", default-features = false }}', root)
        self.assertIn('makepad-notes = { path = ".sources/makepad/apps/notes" }', root)
        shell = (self.root / "crates/shell/Cargo.toml").read_text()
        self.assertIn('makepad-notes = { workspace = true, optional = true }', shell)
        self.assertIn('app-notes = ["dep:makepad-notes"]', shell)
        desktop = (self.root / "desktop/Cargo.toml").read_text()
        self.assertIn('default = ["octos-core", "app-rinx", "app-terminal", "app-hub", "app-notes"]', desktop)
        self.assertIn('app-notes = ["octosense-shell/app-notes"]', desktop)
        self.assertNotIn("app-notes", (self.root / "phone/Cargo.toml").read_text())
        rust = (self.root / native_apps.RUST_FILE).read_text()
        self.assertIn('    #[cfg(feature = "app-notes")]\n    out.push(&makepad_notes::NOTES_MODULE);', rust)

    def test_native_mobile_links_without_the_feature(self):
        rust = native_apps.render_rust(native_apps.validate(self.data))
        self.assertIn('#[cfg(any(feature = "app-reference", native_mobile))]', rust)
        self.assertIn('#[cfg(feature = "app-rinx")]', rust)

    def test_the_phone_turns_on_its_own_optional_dependencies(self):
        blocks = native_apps.package_block(native_apps.validate(self.data), "phone",
                                           (self.root / "phone/Cargo.toml").read_text())["features"]
        self.assertIn('mobile-apps = ["app-reference", "app-hub", "octosense-shell/mobile-apps"]', blocks)
        self.assertIn('app-hub = ["dep:octosense-app-hub-app", "octosense-shell/app-hub"]', blocks)
        self.assertIn('app-rinx = ["octosense-shell/app-rinx"]', blocks)
        self.assertFalse(any(line.startswith("app-terminal") for line in blocks), "Terminal is off on the phone")

    def test_one_revision_per_repository(self):
        self.app("terminal")["source"]["rev"] = "0" * 40
        self.save()
        self.assertEqual(self.run_main("--check"), 2)
        with self.assertRaisesRegex(native_apps.ManifestError, r"one revision per repository"):
            native_apps.generate(self.root)

    def test_missing_markers_are_an_error(self):
        path = self.root / "phone/Cargo.toml"
        path.write_text(path.read_text().replace("# END native-apps: features\n", ""))
        with self.assertRaisesRegex(native_apps.ManifestError, r"phone/Cargo.toml: needs exactly one"):
            native_apps.generate(self.root)

    def test_a_changed_pin_is_updated_through_cargo(self):
        self.app("rinx")["source"]["rev"] = "1" * 40
        self.save()
        with patch.object(native_apps.subprocess, "run") as run:
            self.assertEqual(self.run_main(), 0)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands, [["cargo", "update", "-p", "rinx"], ["cargo", "metadata", "--format-version", "1"]])
        self.assertIn('rinx = { git = "https://github.com/hagency-org/Rinx.git", rev = "' + "1" * 40 + '"',
                      (self.root / "Cargo.toml").read_text())

    def test_nothing_changed_runs_no_cargo(self):
        with patch.object(native_apps.subprocess, "run") as run:
            self.assertEqual(self.run_main(), 0)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
