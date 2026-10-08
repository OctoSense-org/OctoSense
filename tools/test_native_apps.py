import contextlib
import copy
import importlib.util
import io
import json
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("native_apps", ROOT / "tools/native_apps.py")
native_apps = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native_apps)

GENERATED = ["Cargo.toml", "crates/shell/Cargo.toml", "desktop/Cargo.toml", "phone/Cargo.toml",
             native_apps.RUST_FILE, native_apps.AGENTS_FILE, native_apps.MANIFEST, native_apps.DESKTOP_CATALOG,
             native_apps.PROCESS_APPS]


def quiet():
    return contextlib.redirect_stderr(io.StringIO())


class TheRepository(unittest.TestCase):
    def test_every_generated_place_matches_the_manifest(self):
        with quiet(), contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(native_apps.main(["--check"], root=ROOT), 0, out.getvalue())

    def test_the_manifest_declares_todays_native_apps(self):
        apps = native_apps.load(ROOT)
        self.assertEqual([app["id"] for app in apps], ["rinx", "reference", "octobuddy", "sheets", "terminal", "appcard", "apphub",
                                                       "calculator", "clock", "notes", "reminders", "weather",
                                                       "octoscode", "task"])
        hosting = {app["id"]: app["hosting"] for app in apps}
        # Terminal is the only app that runs both linked and as a process
        # (ADR 0004 §2); Task has no module and runs only as one.
        self.assertIsNone(next(app for app in apps if app["id"] == "task")["module"])
        self.assertEqual(hosting["task"]["macos"], "process")
        self.assertEqual(hosting["terminal"]["macos"], "process")
        self.assertEqual(hosting["terminal"]["windows"], "process")
        self.assertEqual(hosting["terminal"]["linux"], "process-if-vulkan")
        for ident in ("apphub", "rinx", "sheets", "reference", "octobuddy", "appcard"):
            self.assertEqual(set(hosting[ident].values()), {"module"}, ident)
        # Non-Vulkan Linux is in-process for every app with a module (a
        # process-only app is not there).
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
    def test_rinx_is_taken_only_as_a_tagged_release(self):
        source = self.app("rinx")["source"]
        del source["tag"]
        self.assertRefused(r"rinx.*tagged release")

    def test_refuses_a_tag_that_is_not_a_release(self):
        self.app("rinx")["source"]["tag"] = "main"
        self.assertRefused(r"release tag vX\.Y\.Z")
        self.app("rinx")["source"]["tag"] = "v1.1.0-rc.2"
        native_apps.validate(self.data)

    def test_a_tag_pin_names_the_tag_and_the_lock_its_commit(self):
        rinx = self.app("rinx")
        self.assertIn('tag = "%s"' % rinx["source"]["tag"], native_apps.pin_line(rinx))
        self.assertNotIn("rev =", native_apps.pin_line(rinx))
        shutil.copy(ROOT / "Cargo.lock", self.root / "Cargo.lock")
        self.assertEqual(native_apps.check_tagged_locks(self.root, native_apps.load(self.root)), [])

    def test_a_moved_tag_fails_the_check(self):
        shutil.copy(ROOT / "Cargo.lock", self.root / "Cargo.lock")
        self.app("rinx")["source"]["rev"] = "f" * 40
        self.save()
        problems = native_apps.check_tagged_locks(self.root, native_apps.load(self.root))
        self.assertTrue(problems and "moved tag" in problems[0], problems)
        self.assertEqual(self.run_main("--check"), 1)

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

    def test_the_agent_block_is_checked(self):
        agent = self.app("terminal")["agent"]
        agent["tools"][0]["name"] = "run"
        self.assertRefused(r"agent\.tools\[0\]: name must be terminal\.<tool>")
        agent["tools"][0]["name"] = "terminal.run"
        agent["tools"][1]["auto_approvable"] = False
        self.assertRefused(r"unknown auto_approvable \(the kernel refuses them\)")
        del agent["tools"][1]["auto_approvable"]
        agent["tools"][1]["risk"] = "harmless"
        self.assertRefused(r"risk must be one of read, act, destructive")
        agent["tools"][1]["risk"] = "read"
        agent["tool_policy"]["type"] = {"confirm": "host", "auto_approvable": False}
        self.assertRefused(r"tool_policy\.type: terminal declares no tool terminal\.type")
        del agent["tool_policy"]["type"]
        agent["budget"] = {"calls_per_turn": 0}
        self.assertRefused(r"agent\.budget\.calls_per_turn must be a positive integer")
        agent["budget"] = {"calls_per_turn": 5, "calls_per_day": 50}
        native_apps.validate(self.data)

    def test_no_app_agent_gets_octos_shell(self):
        for shell in ("shell", "bash", "exec_command", "write_stdin", "group:runtime"):
            self.app("rinx")["agent"]["generic_tools"] = ["read_file", shell]
            self.assertRefused(rf"agent\.generic_tools: {re.escape(shell)} is octos's own shell")

    def test_grants_name_another_apps_shareable_tool(self):
        rinx = self.app("rinx")["agent"]
        rinx["grants"] = [{"app": "terminal", "tool": "terminal.read_screen"}, {"app": "os.mail", "tool": "mail.send"}]
        native_apps.validate(self.data)
        rinx["grants"] = [{"app": "terminal", "tool": "terminal.nope"}]
        self.assertRefused(r"agent\.grants terminal/terminal\.nope: terminal declares no terminal\.nope")
        self.app("terminal")["agent"]["tools"][1]["shareable"] = False
        rinx["grants"] = [{"app": "terminal", "tool": "terminal.read_screen"}]
        self.assertRefused(r"terminal\.read_screen is not shareable")
        rinx["grants"] = [{"app": "rinx", "tool": "rinx.x"}]
        self.assertRefused(r"an app's own tools need no grant")
        rinx["grants"] = [{"app": "nowhere", "tool": "nowhere.x"}]
        self.assertRefused(r"no native app nowhere")

    def test_an_apps_own_agent_may_be_narrowed_to_some_of_its_tools(self):
        """`agent.own_tools`: the tools an app's own agent may call, by
        name, each one of its own; without it, every one of them."""
        apps = native_apps.validate(self.data)
        rust = native_apps.render_rust(apps)
        self.assertIn('own_tools: &["terminal.read_screen", "terminal.read_scrollback"],', rust, "the Terminal's agent reads only")
        self.assertIn('own_tools: &["calculator.eval"],', rust, "an entry that does not narrow keeps every tool")
        terminal = self.app("terminal")["agent"]
        terminal["own_tools"] = ["terminal.nope"]
        self.assertRefused(r"agent\.own_tools: terminal\.nope is not one of terminal's agent\.tools")
        terminal["own_tools"] = ["terminal.read_screen", "terminal.read_screen"]
        self.assertRefused(r"agent\.own_tools names a tool twice")
        terminal["own_tools"] = "terminal.read_screen"
        self.assertRefused(r"agent\.own_tools must be a list of its own tool names")

    def test_only_an_entry_that_names_the_coding_scope_gets_a_kernel_port(self):
        """`kernel`: an app that is itself an octos client gets the coding
        scope only when its entry says so (OctosCode's does); any other value
        is refused."""
        apps = native_apps.validate(self.data)
        self.assertEqual([a["id"] for a in apps if "kernel" in a], ["octoscode"], "OctosCode alone asks for a port")
        rust = native_apps.render_rust(apps)
        self.assertEqual(rust.count("kernel: KernelPort::Coding,"), 1)
        self.assertEqual(rust.count("kernel: KernelPort::None,"), len(apps) - 1, "every other entry has none")
        self.app("calculator")["kernel"] = "coding"
        self.assertEqual(native_apps.render_rust(native_apps.validate(self.data)).count("kernel: KernelPort::Coding,"), 2)
        self.app("calculator")["kernel"] = "host"
        self.assertRefused(r"calculator: kernel must be one of coding")

    def test_the_system_agent_gets_only_an_apps_own_shareable_read_tools(self):
        """`agent.system_tools`: what the system agent may call of an app's
        own tools is named per app, and only its shareable read tools
        qualify; the Terminal's command is never one of them."""
        terminal = self.app("terminal")["agent"]
        terminal["system_tools"] = ["terminal.read_screen"]
        apps = native_apps.validate(self.data)
        self.assertIn('system_tools: &["terminal.read_screen"],', native_apps.render_rust(apps))
        terminal["system_tools"] = ["terminal.run"]
        self.assertRefused(r"agent\.system_tools: terminal\.run must be a shareable read tool")
        terminal["system_tools"] = ["terminal.nope"]
        self.assertRefused(r"agent\.system_tools: terminal\.nope is not one of terminal's agent\.tools")
        terminal["system_tools"] = ["terminal.read_screen", "terminal.read_screen"]
        self.assertRefused(r"agent\.system_tools names a tool twice")
        self.app("terminal")["agent"]["tools"][1]["shareable"] = False
        terminal["system_tools"] = ["terminal.read_screen"]
        self.assertRefused(r"terminal\.read_screen must be a shareable read tool")

    def test_the_agent_block_is_generated(self):
        self.app("rinx")["agent"]["grants"] = [{"app": "terminal", "tool": "terminal.read_screen"}]
        self.app("rinx")["agent"]["budget"] = {"calls_per_day": 99}
        apps = native_apps.validate(self.data)
        rust = native_apps.render_rust(apps)
        self.assertIn('grants: &[("terminal", "terminal.read_screen")],', rust)
        self.assertIn("calls_per_turn: None,\n        calls_per_day: Some(99),", rust)
        self.assertIn('generic_tools: &["read_file",', rust)
        self.assertIn('tools_json: r##"[{"name":"terminal.run",', rust)
        agents = native_apps.render_agents(apps)
        self.assertIn('("rinx", &["octos.session.open", "octos.session.history", "octos.turn.start", "octos.turn.interrupt"]),', agents)
        self.assertNotIn('"sheets"', agents, "an app granted no octos.* services has no line")
        self.assertIn('("terminal", &["octos.session.open",', agents, "the Terminal's own agent (read tools only, agent.own_tools)")

    def test_the_shipped_agent_blocks(self):
        rinx = self.app("rinx")["agent"]
        self.assertIn("ask_user_question", rinx["generic_tools"], "Rinx's agent may ask the person (ADR 0004 §6)")
        terminal = self.app("terminal")["agent"]
        self.assertEqual([t["name"] for t in terminal["tools"]], ["terminal.run", "terminal.read_screen", "terminal.read_scrollback"])
        self.assertEqual({t["name"]: t["risk"] for t in terminal["tools"]},
                         {"terminal.run": "destructive", "terminal.read_screen": "read", "terminal.read_scrollback": "read"})
        for app in self.data["apps"]:
            for tool in app["agent"].get("generic_tools", []):
                self.assertNotIn(tool, native_apps.OCTOS_SHELL, app["id"])

    def test_refuses_unknown_keys(self):
        self.app("reference")["hosted"] = "yes"
        self.assertRefused(r"reference: unknown hosted")

    def test_the_display_name_is_checked_and_generated(self):
        rust = native_apps.render_rust(native_apps.validate(self.data))
        self.assertIn('id: "octobuddy",\n        name: Some("OctoBuddy"),', rust)
        self.assertIn('id: "rinx",\n        name: None,', rust)
        for bad in ("", " OctoBuddy", "x" * 41, 7):
            self.app("octobuddy")["name"] = bad
            self.assertRefused(r"octobuddy: name must be a non-empty string")


class Generation(Fixture):
    def test_the_generated_rust_parses(self):
        """Every generated Rust file is valid syntax: a field without its
        comma once reached a pull request (`name: Option<&'static str>`)."""
        apps = native_apps.validate(self.data)
        for label, rust in (("native_apps.rs", native_apps.render_rust(apps)),
                            ("native_agents.rs", native_apps.render_agents(apps))):
            for line in rust.splitlines():
                if re.fullmatch(r"    pub \w+: .*", line):
                    self.assertTrue(line.endswith(","), f"{label}: {line.strip()}")
            rustfmt = shutil.which("rustfmt")
            if rustfmt is None:
                continue
            run = subprocess.run([rustfmt, "--edition", "2021", "--emit", "stdout"], input=rust,
                                 capture_output=True, text=True)
            self.assertNotIn("error", run.stderr, f"{label}: {run.stderr}")
            self.assertEqual(run.returncode, 0, f"{label}: {run.stderr}")

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
        extra.update({"id": "image", "crate": "makepad-image", "module": "makepad_image::IMAGE_MODULE", "bin": "image",
                      "shells": {"desktop": "default", "phone": "off"}, "native_mobile": "feature"})
        extra["source"]["local"] = ".sources/makepad/apps/image"
        self.data["apps"].append(extra)
        self.save()
        self.assertEqual(self.run_main("--no-lock"), 0)
        root = (self.root / "Cargo.toml").read_text()
        rev = extra["source"]["rev"]  # the Makepad pin, whatever it is today
        self.assertIn(f'makepad-image = {{ git = "https://github.com/OctoSense-org/makepad.git", rev = "{rev}", default-features = false }}', root)
        self.assertIn('makepad-image = { path = ".sources/makepad/apps/image" }', root)
        shell = (self.root / "crates/shell/Cargo.toml").read_text()
        self.assertIn('makepad-image = { workspace = true, optional = true }', shell)
        self.assertIn('app-image = ["dep:makepad-image"]', shell)
        desktop = (self.root / "desktop/Cargo.toml").read_text()
        default = next(line for line in desktop.splitlines() if line.startswith("default = "))
        self.assertIn('"app-image"', default, "a desktop default app is in the package's default features")
        self.assertIn('app-image = ["octosense-shell/app-image"]', desktop)
        self.assertNotIn("app-image", (self.root / "phone/Cargo.toml").read_text())
        rust = (self.root / native_apps.RUST_FILE).read_text()
        self.assertIn('    #[cfg(feature = "app-image")]\n    out.push(&makepad_image::IMAGE_MODULE);', rust)

    def process_only(self):
        """Task: the manifest's process-only app (`module: null`)."""
        app = self.app("task")
        self.assertIsNone(app["module"])
        return app

    def test_a_process_only_app_is_built_never_linked(self):
        """`module: null` (ADR 0004 §2): the app runs only as its own
        desktop process. The process-app crate builds it; no shell links
        it, and it is not there where there are no processes."""
        self.process_only()
        self.save()
        self.assertEqual(self.run_main("--no-lock"), 0)
        self.assertIn('makepad-task = { workspace = true }', (self.root / "crates/process-apps/Cargo.toml").read_text())
        self.assertNotIn("makepad-task", (self.root / "crates/shell/Cargo.toml").read_text())
        self.assertNotIn("app-task", (self.root / "desktop/Cargo.toml").read_text())
        rust = (self.root / native_apps.RUST_FILE).read_text()
        self.assertNotIn('feature = "app-task"', rust, "nothing links it")
        self.assertIn('bin: Some("task"),\n        macos: Hosting::Process,', rust)
        self.assertIn("android: Hosting::None,", rust)
        self.assertIn("wasm: Hosting::None,", rust)

    def test_a_process_only_app_is_checked(self):
        app = self.process_only()
        native_apps.validate(self.data)
        app["bin"] = None
        self.assertRefused(r"task: a process-only app \(module null\) needs a bin")
        app["bin"] = "task"
        app["hosting"]["android"] = "module"
        self.assertRefused(r"hosting\.android: a process-only app \(module null\) has no module")
        app["hosting"]["android"] = "process"
        self.assertRefused(r"hosting\.android: android has no processes")
        app["hosting"]["android"] = "none"
        app["shells"]["desktop"] = "default"
        self.assertRefused(r"task: a process-only app is linked by no shell: shells must be off")
        app["shells"]["desktop"] = "off"
        self.app("terminal")["hosting"]["android"] = "none"
        self.assertRefused(r"terminal: hosting\.android: 'none' is for a process-only app")

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
        self.app("rinx")["source"].update(tag="v1.0.1", rev="1" * 40)
        self.save()
        with patch.object(native_apps.subprocess, "run") as run:
            self.assertEqual(self.run_main(), 0)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands, [["cargo", "update", "-p", "rinx"], ["cargo", "metadata", "--format-version", "1"]])
        self.assertIn('rinx = { git = "https://github.com/hagency-org/Rinx.git", tag = "v1.0.1"',
                      (self.root / "Cargo.toml").read_text())

    def catalog(self):
        path = self.root / native_apps.DESKTOP_CATALOG
        return path, json.loads(path.read_text())

    def test_the_desktop_catalog_names_what_a_process_launch_builds(self):
        path, rows = self.catalog()
        terminal = next(row for row in rows if row["id"] == "terminal")
        terminal["bin"] = "makepad-terminal"
        terminal["package"] = "terminal"
        path.write_text(json.dumps(rows))
        with contextlib.redirect_stdout(io.StringIO()) as out, quiet():
            self.assertEqual(native_apps.main(["--check"], root=self.root), 1)
        self.assertIn("terminal's bin is 'makepad-terminal', native-apps.json says 'terminal'", out.getvalue())
        self.assertIn("terminal's package is 'terminal', native-apps.json says 'makepad-terminal'", out.getvalue())

    def test_a_process_app_needs_a_catalog_row_from_its_own_source(self):
        path, rows = self.catalog()
        sheets = next(row for row in rows if row["id"] == "sheets")
        sheets.pop("source")
        sheets["manifest"] = "../../apps/sheets/Cargo.toml"
        reference = next(row for row in rows if row["id"] == "reference")
        reference["manifest"] = "../../apps/elsewhere/Cargo.toml"
        path.write_text(json.dumps([row for row in rows if row["id"] != "terminal"]))
        problems = native_apps.catalog_problems(self.root, native_apps.load(self.root))
        self.assertIn(f"{native_apps.DESKTOP_CATALOG} has no row for terminal, which runs as a process on a desktop", problems)
        self.assertTrue(any("sheets comes from Makepad" in p for p in problems), problems)
        self.assertTrue(any("reference's manifest should be apps/reference/Cargo.toml" in p for p in problems), problems)
        # A native app with no bin has no process form, so no row either.
        rows.append({"id": "rinx", "label": "Rinx", "source": "makepad", "package": "rinx", "bin": "rinx"})
        path.write_text(json.dumps(rows))
        problems = native_apps.catalog_problems(self.root, native_apps.load(self.root))
        self.assertTrue(any("rinx has a row but no bin" in p for p in problems), problems)

    def test_the_shell_reads_each_process_apps_package_from_the_manifest(self):
        rust = native_apps.render_rust(native_apps.load(self.root))
        self.assertIn('"terminal" => Some("makepad-terminal"),', rust)
        self.assertIn('"reference" => Some("octosense-reference"),', rust)
        self.assertNotIn('"rinx" => Some(', rust, "no bin: no process package")

    def test_the_process_build_package_names_every_binary_with_its_features(self):
        blocks = native_apps.process_blocks(native_apps.load(self.root))["process deps"]
        self.assertIn('makepad-sheets = { workspace = true, features = ["standalone"] }', blocks)
        self.assertIn("makepad-terminal = { workspace = true }", blocks)
        self.assertIn("octosense-reference = { workspace = true }", blocks)
        self.assertFalse(any(line.startswith("rinx ") for line in blocks), "no bin: no process build")
        self.app("rinx")["bin_features"] = ["x"]
        self.assertRefused(r"rinx: bin_features without a bin")

    def test_nothing_changed_runs_no_cargo(self):
        with patch.object(native_apps.subprocess, "run") as run:
            self.assertEqual(self.run_main(), 0)
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
