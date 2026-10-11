#!/usr/bin/env python3
"""Exercise a real model-authored Task Planner through the Studio instrument.

Only the separate developer test package is in scope. This harness never
writes application source or changes task storage to manufacture success.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "tools/studio-flow/task-planner-acceptance.json"
_spec = importlib.util.spec_from_file_location("studio_device_probe", ROOT / "tools/studio-device-probe.py")
_probe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_probe)
ProbeError = _probe.ProbeError
Device = _probe.Device


def widgets(snapshot):
    if not isinstance(snapshot, dict) or not isinstance(snapshot.get("widgets"), list):
        raise ProbeError("inspect result needs a native widgets array")
    return [node for node in snapshot["widgets"] if isinstance(node, dict) and node.get("visible") is True]


def node_text(node):
    return str(node.get("text") or node.get("value") or "").strip()


def input_value(node):
    """Read editable content, never the placeholder exposed as native text."""
    if "TextInput" not in str(node.get("type", "")) or not isinstance(node.get("value"), str):
        raise ProbeError("selected TextInput must expose its actual string value")
    return node["value"]


def bounds(node):
    value = node.get("rect")
    if not isinstance(value, list) or len(value) != 4 or not all(isinstance(n, (int, float)) for n in value):
        raise ProbeError("native widget has no measured rectangle")
    if value[2] <= 0 or value[3] <= 0:
        raise ProbeError("native widget has empty bounds")
    return value


def unique_node(snapshot, *, source_id=None, text=None, kind=None, enabled=True):
    nodes = widgets(snapshot)
    if source_id is not None:
        nodes = [node for node in nodes if str(node.get("id", "")) == source_id
                 or str(node.get("id", "")).endswith(("/" + source_id, "." + source_id))]
    if text is not None:
        nodes = [node for node in nodes if node_text(node) == text]
    if kind is not None:
        nodes = [node for node in nodes if kind.lower() in str(node.get("type", "")).lower()]
    if len(nodes) != 1:
        raise ProbeError(f"selector must match one visible native widget, found {len(nodes)}: "
                         f"id={source_id!r} text={text!r} type={kind!r}")
    node = nodes[0]
    bounds(node)
    if enabled and node.get("enabled") is not True:
        raise ProbeError("selected native control is disabled")
    return node


def task_action(snapshot, title, action):
    title_node = unique_node(snapshot, text=title, enabled=False)
    title_rect = bounds(title_node)
    row_center = title_rect[1] + title_rect[3] / 2
    candidates = []
    for node in widgets(snapshot):
        if node_text(node) != action or node.get("enabled") is not True:
            continue
        rect = bounds(node)
        if rect[1] <= row_center <= rect[1] + rect[3]:
            candidates.append(node)
    if len(candidates) != 1:
        raise ProbeError(f"cannot identify one {action} control beside task {title!r}")
    return candidates[0]


def assert_tasks(snapshot, expected, all_titles):
    shown = {node_text(node) for node in widgets(snapshot)}
    actual = [title for title in all_titles if title in shown]
    if set(actual) != set(expected):
        raise ProbeError(f"visible tasks differ: expected {expected!r}, found {actual!r}")


def assert_count(snapshot, source_id, number):
    node = unique_node(snapshot, source_id=source_id, enabled=False)
    if not re.search(rf"\b{number}\s+active\b", node_text(node), re.IGNORECASE):
        raise ProbeError(f"expected {number} active; native status is {node_text(node)!r}")


def verify_controls(snapshot, contract):
    names = contract["selectors"]
    checks = []
    for key in ("title_input", "add", "all", "active", "done"):
        node = unique_node(snapshot, source_id=names[key])
        rect = bounds(node)
        if rect[3] < contract["minimum_control_height_logical"]:
            raise ProbeError(f"{names[key]} is only {rect[3]} logical pixels high")
        checks.append({"selector": names[key], "widget_id": node["id"], "rect": rect})
    return checks


def safe_relative(value):
    if not isinstance(value, str) or not value or value.startswith("/") or "\\" in value:
        raise ProbeError("expected a relative workspace path")
    if any(part in ("", ".", "..") for part in value.split("/")):
        raise ProbeError("workspace path contains traversal or empty components")
    return value


def full_inspection(device, workspace, instance, compact):
    encoded = json.dumps(compact, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(encoded) > 3800:
        raise ProbeError("inspect response exceeds the model-visible 3800-byte budget")
    path = safe_relative(compact.get("snapshot_path"))
    raw = device.read(workspace + "/" + path, 1024 * 1024)
    if raw is None:
        raise ProbeError("inspect returned a missing full snapshot artifact")
    if len(raw) > 1024 * 1024:
        raise ProbeError("full snapshot artifact exceeds 1MiB")
    try:
        full = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ProbeError("full snapshot artifact is not complete UTF-8 JSON") from error
    if not isinstance(full, dict) or full.get("instance_id") != instance:
        raise ProbeError("full snapshot artifact belongs to another instance")
    for key in ("instance_id", "path", "width", "height", "settled"):
        if key not in full or full[key] != compact.get(key):
            raise ProbeError(f"full snapshot and compact response disagree on {key}")
    widgets(full.get("snapshot"))
    return full


class Studio:
    def __init__(self, device, workspace, evidence_dir, timeout):
        self.device, self.workspace, self.evidence_dir = device, workspace, evidence_dir
        self.timeout = timeout
        self.calls = []
        self.inspections = 0
        self.instance = None
        self.snapshot = None

    def start(self):
        import shlex
        self.device.stop()
        self.device.private("umask 077; mkdir -p " + shlex.join([
            self.workspace + "/requests", self.workspace + "/responses"]))
        config_path = self.workspace + "/studio-flow-config.json"
        self.device.write(config_path, json.dumps({"workspace": self.workspace}).encode())
        config = json.dumps({"test_actions": ["studio-flow:" + config_path]}, separators=(",", ":"))
        launch = self.device.shell("am", "start", "-n", self.device.package + "/.MakepadApp",
                                   "--es", "makepad.APP_CONFIG", config)
        if b"Error:" in launch.stdout or b"Error:" in launch.stderr:
            raise ProbeError("studio flow activity failed to start")
        self.instance = None
        self.snapshot = None

    def call(self, name, args):
        import shlex
        request_id = str(uuid.uuid4())
        request = {"name": name, "args": args}
        payload = json.dumps(request).encode()
        if len(payload) > 16 * 1024:
            raise ProbeError("test request exceeds 16KiB")
        temporary = self.workspace + "/requests/" + request_id + ".pending"
        destination = self.workspace + "/requests/" + request_id + ".json"
        self.device.write(temporary, payload)
        self.device.private("mv " + shlex.join([temporary, destination]))
        started = time.monotonic()
        response = None
        while time.monotonic() - started < self.timeout:
            raw = self.device.read(self.workspace + "/responses/" + request_id + ".json", 1024 * 1024)
            if raw:
                try:
                    response = json.loads(raw)
                    break
                except json.JSONDecodeError:
                    pass
            time.sleep(0.1)
        record = {"request_id": request_id, "request": request, "reply": response,
                  "elapsed_seconds": round(time.monotonic() - started, 3)}
        self.calls.append(record)
        (self.evidence_dir / "tool-calls.json").write_text(json.dumps(self.calls, indent=2) + "\n")
        if not isinstance(response, dict):
            raise ProbeError(f"{name}: no complete response before timeout")
        if response.get("ok") is not True:
            raise ProbeError(f"{name}: {response.get('error')}")
        result = response.get("data")
        if not isinstance(result, dict):
            raise ProbeError(f"{name}: expected object result")
        return result

    def open(self, **args):
        result = self.call("studio.open", args)
        instance = result.get("instance_id")
        if not isinstance(instance, str) or not instance:
            raise ProbeError("studio.open returned no instance_id")
        self.instance = instance
        return result

    def close(self):
        if self.instance is not None:
            self.call("studio.close", {"instance_id": self.instance})
            self.instance = None
            self.snapshot = None

    def inspect(self, label):
        compact = self.call("studio.inspect", {"instance_id": self.instance})
        result = full_inspection(self.device, self.workspace, self.instance, compact)
        snapshot = result["snapshot"]
        self.snapshot = snapshot
        self.inspections += 1
        stem = f"{self.inspections:03d}-{label}"
        (self.evidence_dir / f"{stem}.json").write_text(json.dumps(result, indent=2) + "\n")
        (self.evidence_dir / f"{stem}.compact.json").write_text(json.dumps(compact, indent=2) + "\n")
        path = safe_relative(result.get("path"))
        png = self.device.read(self.workspace + "/" + path, _probe.PNG_LIMIT)
        if png is None:
            raise ProbeError("inspect returned a missing PNG")
        metadata = _probe.png_metadata(png)
        (self.evidence_dir / f"{stem}.png").write_bytes(png)
        if metadata["width"] < 240 or metadata["height"] < 240:
            raise ProbeError("full-app inspection PNG is implausibly small")
        if (metadata["width"], metadata["height"]) != (result["width"], result["height"]):
            raise ProbeError("inspection PNG dimensions disagree with its full snapshot")
        checks = snapshot.get("checks")
        if not isinstance(checks, dict) or checks.get("pass") is not True:
            raise ProbeError(f"native layout checks did not pass: {checks}")
        return snapshot

    def input(self, node, action="tap", text=None):
        args = {"instance_id": self.instance, "widget_id": node.get("selector", node["id"]), "action": action}
        if text is not None:
            args["text"] = text
        return self.call("studio.input", args)

    def tap(self, source_id, label):
        self.input(unique_node(self.snapshot, source_id=source_id))
        return self.inspect(label)

    def add(self, title, selectors, label):
        field = unique_node(self.snapshot, source_id=selectors["title_input"])
        self.input(field, "text", title)
        self.inspect(label + "-typed")
        return self.tap(selectors["add"], label + "-added")


def exercise(studio, contract, label):
    names = contract["selectors"]
    titles = contract["fixture_titles"]
    snap = studio.inspect(label + "-empty")
    controls = verify_controls(snap, contract)
    assert_tasks(snap, [], titles)
    assert_count(snap, names["count"], 0)
    unique_node(snap, text="No tasks yet", enabled=False)
    snap = studio.tap(names["add"], label + "-blank")
    assert_tasks(snap, [], titles)
    error = unique_node(snap, source_id=names["validation"], enabled=False)
    if not node_text(error):
        raise ProbeError("blank Add did not explain validation")
    for index, title in enumerate(titles):
        snap = studio.add(title, names, label + f"-task-{index + 1}")
        assert_tasks(snap, titles[:index + 1], titles)
        assert_count(snap, names["count"], index + 1)
        if input_value(unique_node(snap, source_id=names["title_input"])) != "":
            raise ProbeError("Add did not clear the text input")
    studio.input(task_action(snap, contract["completed_title"], "Complete"))
    snap = studio.inspect(label + "-completed")
    assert_count(snap, names["count"], 2)
    for filter_name in ("active", "done", "all"):
        snap = studio.tap(names[filter_name], label + "-" + filter_name)
        assert_tasks(snap, contract["expected_before_restart"][filter_name], titles)
    snap = studio.tap(names["done"], label + "-done-before-reopen")
    studio.input(task_action(snap, contract["completed_title"], "Reopen"))
    snap = studio.inspect(label + "-task-reopened")
    assert_tasks(snap, [], titles)
    assert_count(snap, names["count"], 3)
    snap = studio.tap(names["all"], label + "-all-before-persist")
    studio.input(task_action(snap, contract["completed_title"], "Complete"))
    snap = studio.inspect(label + "-persist-state")
    assert_count(snap, names["count"], 2)
    return {"controls": controls, "state": contract["expected_before_restart"]}


def assert_persisted(studio, contract, label):
    snap = studio.inspect(label + "-all")
    names = contract["selectors"]
    assert_count(snap, names["count"], 2)
    assert_tasks(snap, contract["fixture_titles"], contract["fixture_titles"])
    snap = studio.tap(names["done"], label + "-done")
    assert_tasks(snap, [contract["completed_title"]], contract["fixture_titles"])
    studio.tap(names["all"], label + "-all-restored")


def exercise_multilingual(studio, contract):
    title = contract["multilingual_title"]
    names = contract["selectors"]
    snapshot = studio.add(title, names, "multilingual")
    unique_node(snapshot, text=title, enabled=False)
    assert_count(snapshot, names["count"], 3)
    studio.close()
    studio.open(app_id=contract["app_id"])
    snapshot = studio.inspect("multilingual-reopened")
    unique_node(snapshot, text=title, enabled=False)
    assert_count(snapshot, names["count"], 3)
    return {"status": "passed", "title": title,
            "verified": "exact native text after input and installed-app reopen",
            "glyph_review": "required; exact widget text alone does not prove readable glyphs"}


def keyboard_diagnostics(device):
    """Record only visibility/ownership; app PNGs exclude the Android IME."""
    result = device.shell("dumpsys", "input_method", check=False)
    text = result.stdout.decode(errors="replace")
    shown = re.findall(r"\b(?:mInputShown|mIsInputViewShown)=(true|false)", text)
    owners = re.findall(r"\bpackageName=([A-Za-z0-9_.]+)", text)
    visible = bool(shown) and all(value == "true" for value in shown)
    owned = device.package in owners
    return {"status": "observed" if result.returncode == 0 and visible and owned else "unverified",
            "input_shown": visible if shown else None,
            "test_package_reported": owned,
            "capture_scope": "app texture only; Android keyboard pixels are not captured",
            "visual_keyboard_review": "not established by app PNG"}


def generation_receipt(path):
    raw = path.read_bytes()
    data = json.loads(raw)
    if data.get("authorship") != "model" or not isinstance(data.get("model"), str) or not data["model"].strip():
        raise ProbeError("generation receipt must identify model authorship and model")
    for field in ("bundle_digest", "transcript_sha256"):
        if not isinstance(data.get(field), str) or not re.fullmatch(r"[0-9a-f]{64}", data[field]):
            raise ProbeError(f"generation receipt needs a 64-character lowercase {field}")
    return {key: data[key] for key in ("authorship", "model", "bundle_digest", "transcript_sha256")} | {
        "receipt_sha256": hashlib.sha256(raw).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--package", type=_probe.test_package, default=_probe.DEFAULT_PACKAGE)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--workspace", required=True, help="absolute authoring workspace inside the test package's files/studio-fixture directory")
    parser.add_argument("--bundle-path", required=True, help="model-authored bundle, relative to workspace")
    parser.add_argument("--generation-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="new local evidence directory")
    parser.add_argument("--timeout", type=float, default=40)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.timeout < 30:
        parser.error("--timeout must be at least 30 seconds")
    try:
        safe_relative(args.bundle_path)
        allowed = [f"/data/user/0/{args.package}/files/studio-fixture/", f"/data/data/{args.package}/files/studio-fixture/"]
        if not any(args.workspace.startswith(prefix) for prefix in allowed):
            raise ProbeError("workspace must be inside the studio test package's files/studio-fixture folder")
        safe_relative(args.workspace[1:])
        contract = json.loads(CONTRACT.read_text())
        generation = generation_receipt(args.generation_receipt)
    except (OSError, ValueError, ProbeError) as error:
        parser.error(str(error))
    plan = {"serial": args.serial, "package": args.package, "workspace": args.workspace,
            "bundle_path": args.bundle_path, "app_id": contract["app_id"], "generation": generation,
            "contract_sha256": hashlib.sha256(CONTRACT.read_bytes()).hexdigest(),
            "direct_source_or_app_storage_writes": False}
    if args.dry_run:
        print(json.dumps(plan, indent=2))
        return 0
    args.output.mkdir(parents=True, exist_ok=False)
    evidence = {**plan, "status": "failed", "started_unix": int(time.time()), "checks": {},
                "visual_review": {"status": "required", "items": ["readability", "clipping", "overlap", "contrast", "orientation", "multilingual glyphs", "keyboard"]},
                "fault_checks": {name: "not run; requires isolated fault injection" for name in contract["fault_checks"]}}
    device = Device(args.adb, args.serial, args.package)
    studio = Studio(device, args.workspace, args.output, args.timeout)
    try:
        if device.call("get-state").stdout.strip() != b"device":
            raise ProbeError("authorized device unavailable")
        state = device.read("files/.octosense/dev-mode.json", 65536)
        if not state or json.loads(state).get("scope") != "all":
            raise ProbeError("configure developer mode in this test profile before running the flow")
        studio.start()
        checked = studio.call("studio.bundle_check", {"bundle_path": args.bundle_path})
        evidence["admission"] = checked
        if checked.get("digest") != generation["bundle_digest"] or checked.get("app_id") != contract["app_id"]:
            raise ProbeError("checked app identity/digest does not match the model generation receipt")
        if checked.get("source_modified") is not False or checked.get("publisher_signed") is not False:
            raise ProbeError("expected source-preserving unsigned developer preparation")
        evidence["checks"]["model_bundle_digest_binding"] = "passed; authorship evidence requires transcript review"
        studio.open(bundle_path=args.bundle_path)
        evidence["checks"]["preview_interactions"] = exercise(studio, contract, "preview")
        studio.close()
        studio.open(bundle_path=args.bundle_path)
        snap = studio.inspect("preview-reset")
        assert_tasks(snap, [], contract["fixture_titles"])
        assert_count(snap, contract["selectors"]["count"], 0)
        evidence["checks"]["preview_storage_discarded"] = "passed"
        studio.close()
        installed = studio.call("studio.install", {"bundle_path": args.bundle_path})
        if installed.get("digest") != checked["digest"] or installed.get("installed") is not True:
            raise ProbeError("developer install did not preserve checked bundle identity")
        evidence["installation"] = installed
        studio.open(app_id=contract["app_id"])
        evidence["checks"]["installed_interactions_and_separate_storage"] = exercise(studio, contract, "installed")
        studio.close()
        studio.open(app_id=contract["app_id"])
        assert_persisted(studio, contract, "installed-reopen")
        evidence["checks"]["installed_close_reopen"] = "passed"
        # An abrupt process stop while open proves persistence is app-owned,
        # rather than an in-memory preview or a graceful-close-only save.
        studio.start()
        studio.open(app_id=contract["app_id"])
        assert_persisted(studio, contract, "process-restart")
        evidence["checks"]["installed_process_restart"] = "passed"
        names = contract["selectors"]
        evidence["checks"]["multilingual_native_input_and_restart"] = exercise_multilingual(studio, contract)
        studio.add(contract["long_title"], names, "long-title")
        # More rows than can fit at the required minimum height makes scroll
        # acceptance meaningful; sending an event to a short list proves little.
        list_node = unique_node(studio.snapshot, source_id=names["list"])
        required_rows = int(bounds(list_node)[3] / contract["minimum_control_height_logical"]) + 3
        extra_count = max(1, required_rows - 5)
        if extra_count > 20:
            raise ProbeError("unexpectedly large list viewport; inspect display scale")
        extra_titles = [f"Scroll check {index + 1:02d}" for index in range(extra_count)]
        for index, title in enumerate(extra_titles):
            studio.add(title, names, f"scroll-row-{index + 1:02d}")
        def scroll(delta, label):
            node = unique_node(studio.snapshot, source_id=names["list"])
            studio.call("studio.input", {"instance_id": studio.instance,
                        "widget_id": node.get("selector", node["id"]),
                        "action": "scroll", "delta_y": delta})
            return studio.inspect(label)
        top = scroll(-2000, "scroll-top")
        unique_node(top, text=contract["fixture_titles"][0], enabled=False)
        bottom = scroll(2000, "scroll-bottom")
        unique_node(bottom, text=extra_titles[-1], enabled=False)
        if any(node_text(node) == contract["fixture_titles"][0] for node in widgets(bottom)):
            raise ProbeError("scroll did not move the first row out of the viewport")
        assert_count(bottom, names["count"], 4 + extra_count)
        evidence["checks"]["native_scroll"] = {"status": "passed", "extra_rows": extra_count,
            "first_row_at_top": contract["fixture_titles"][0], "last_row_at_bottom": extra_titles[-1]}
        # Typing targets the native editor. Its app-only capture cannot contain
        # the OS keyboard, so visibility/owner evidence is explicitly separate.
        studio.input(unique_node(bottom, source_id=names["title_input"]), "text", "Keyboard layout check")
        studio.inspect("keyboard-layout")
        evidence["keyboard"] = keyboard_diagnostics(device)
        evidence["checks"]["long_title"] = "native input exercised; review captures for wrapping and clipping"
        studio.close()
        evidence["status"] = "functional_passed_visual_review_required"
    except Exception as error:
        evidence["error"] = str(error)
    finally:
        try:
            studio.close()
        except Exception as error:
            evidence["close_error"] = str(error)
            evidence["status"] = "failed"
        try:
            device.stop()
            device.shell("input", "keyevent", "KEYCODE_HOME")
        except Exception as error:
            evidence["cleanup_error"] = str(error)
            evidence["status"] = "failed"
        evidence["tool_call_count"] = len(studio.calls)
        evidence["finished_unix"] = int(time.time())
        (args.output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"status": evidence["status"], "evidence": str(args.output / "evidence.json")}, indent=2))
    return 0 if evidence["status"] == "functional_passed_visual_review_required" else 1


if __name__ == "__main__":
    raise SystemExit(main())
