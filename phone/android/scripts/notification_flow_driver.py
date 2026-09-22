"""App-window input and captures for the real notification flow fixture."""
import json
import http.client
import re
import time
import urllib.parse
import urllib.error
import urllib.request
import uuid


class NotificationFlowDriver:
    def __init__(self, command, output, report, visible_seconds, skip_captures=False):
        self.command, self.output, self.report = command, output, report
        self.visible_seconds = visible_seconds
        self.skip_captures = skip_captures
        self.previous = set(self.remotes())
        self.base = self.forward = None
        self.report.update(ui_states={}, captures=[], capture_scope="owned OctoSense window only")

    def remotes(self):
        return re.findall(r"--remote pid=(\d+) port=(\d+) token=([a-f0-9-]+)",
                          self.command("logcat", "-d", "-s", "OctoSenseValidation:I", "*:S"))

    def request(self, route):
        try:
            with urllib.request.urlopen(self.base + route, timeout=10) as response:
                return response.read()
        except urllib.error.URLError as error:
            # A refused socket dispatched no HTTP request. Restore only our
            # missing forward; never replay input after an uncertain transfer.
            if getattr(error.reason, "errno", None) not in (61, 111) or not self.forward:
                raise
            if any("tcp:" + self.forward in line.split() for line in self.command("forward", "--list").splitlines()):
                raise
            self.command("forward", "tcp:" + self.forward, "tcp:" + self.device_port)
            self.report["forward_reconnections"] = self.report.get("forward_reconnections", 0) + 1
            with urllib.request.urlopen(self.base + route, timeout=10) as response:
                return response.read()

    @staticmethod
    def launcher(value):
        return value["widgets"]["launcher"]

    def state(self, name, predicate, timeout=25):
        deadline = time.monotonic() + timeout
        value = None
        while time.monotonic() < deadline:
            try:
                value = json.loads(self.request("/status"))
                assert value["pid"] == self.report["owned_home_pid"]
                if predicate(value):
                    self.report["ui_states"][name] = value
                    print("Verified UI: " + name, flush=True)
                    return value
            except (OSError, ValueError, http.client.HTTPException):
                pass
            time.sleep(0.15)
        self.report["ui_states"][name + "-timeout"] = value
        raise AssertionError("UI state timeout: " + name)

    def renderer(self, name, predicate):
        deadline = time.monotonic() + 25
        value = None
        while time.monotonic() < deadline:
            token = str(uuid.uuid4())
            assert json.loads(self.request("/notification/probe?token=" + token))["queued"]
            value = self.state(name + "-probe", lambda v: (self.launcher(v).get("notification_renderer") or {}).get("token") == token)
            if predicate(self.launcher(value)["notification_renderer"]):
                self.report["ui_states"][name] = value
                return value
            time.sleep(0.15)
        raise AssertionError("Renderer timeout: " + name)

    @staticmethod
    def point(value, bounds, fraction=0.5):
        probe = value["widgets"]["launcher"]["notification_renderer"]
        scale = value["width"] / probe["logical_width"]
        offset = value["surface_capture_offset"]
        assert bounds is not None and offset is not None and 0.5 <= scale <= 6
        return {"x": (bounds[0] + bounds[2] * fraction) * scale + offset[0],
                "y": (bounds[1] + bounds[3] / 2) * scale + offset[1]}

    def tap(self, point):
        self.request("/tap?" + urllib.parse.urlencode(point))
        time.sleep(0.2)

    def capture(self, filename, surface=False):
        if self.skip_captures:
            self.report.setdefault("skipped_captures", []).append(filename)
            return
        capture = json.loads(self.request("/capture/start?layer=" + ("surface" if surface else "window")))
        assert 0 < capture["bytes"] <= 64_000_000
        data = bytearray()
        try:
            while len(data) < capture["bytes"]:
                length = min(16384, capture["bytes"] - len(data))
                part = self.request("/capture/chunk?" + urllib.parse.urlencode({
                    "capture": capture["capture"], "offset": len(data), "length": length}))
                assert len(part) == length, "Truncated owned capture chunk"
                data.extend(part)
            assert data.startswith(b"\x89PNG\r\n\x1a\n")
            (self.output / filename).write_bytes(data)
            self.report["captures"].append(filename)
        finally:
            self.request("/capture/release")

    def reveal(self, name):
        value = self.renderer(name + "-card", lambda r: len(r["notes"]) == 1 and r["shade_open"] >= 0.99)
        note = self.launcher(value)["notification_renderer"]["notes"][0]
        if not note["revealed"]:
            start = self.point(value, note["card_bounds"], 0.85)
            end = self.point(value, note["card_bounds"], 0.15)
            self.request("/swipe?" + urllib.parse.urlencode({"x1": start["x"], "y1": start["y"], "x2": end["x"], "y2": end["y"]}))
            time.sleep(0.6)
        return self.renderer(name + "-actions", lambda r: len(r["notes"]) == 1 and r["notes"][0]["revealed"]
                             and r["notes"][0]["actions"][0]["bounds"] is not None and r["notes"][0]["dismiss_bounds"] is not None)

    def open_reply(self, name):
        value = self.reveal(name)
        note = self.launcher(value)["notification_renderer"]["notes"][0]
        assert len(note["actions"]) == 1 and note["actions"][0]["label"] == "Reply"
        self.tap(self.point(value, note["actions"][0]["bounds"]))
        return self.state(name + "-editor", lambda v: self.launcher(v)["reply_editor"]["visible"]
                          and self.launcher(v)["reply_editor"].get("editor_focused")
                          and self.launcher(v)["reply_editor"].get("ime_visible"))

    def text(self, value):
        self.request("/flow/text?" + urllib.parse.urlencode({"value": value}))

    def hold(self, label):
        print("Visible on OnePlus 6: " + label + " (" + str(self.visible_seconds) + " seconds)", flush=True)
        time.sleep(self.visible_seconds)

    def run(self, log_path, process, listener):
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline:
            if "fixture_phase=ready_for_grant" in log_path.read_text():
                break
            assert process.poll() is None, "UI instrumentation exited before ready"
            time.sleep(0.2)
        else:
            raise AssertionError("UI fixture did not reach grant boundary")
        remotes = [entry for entry in self.remotes() if entry not in self.previous]
        assert remotes, "Owned Home remote missing"
        pid, port, token = remotes[-1]
        self.device_port = port
        self.report["owned_home_pid"] = int(pid)
        self.forward = self.command("forward", "tcp:0", "tcp:" + port)
        self.base = "http://127.0.0.1:" + self.forward + "/" + token
        self.state("initial-access-revoked", lambda v: v["notification_flow"]["observed"] and not v["notification_flow"]["accessible"])
        listener(True)
        self.report["phases"].append("ready_for_grant")
        self.state("listener-connected", lambda v: v["notification_flow"]["accessible"])
        self.request("/flow/post")
        initial = self.renderer("fixture-delivered", lambda r: len(r["notes"]) == 1 and r["open_bounds"] is not None)
        self.tap(self.point(initial, self.launcher(initial)["notification_renderer"]["open_bounds"]))
        self.renderer("shade-visible", lambda r: r["shade_open"] >= 0.99 and len(r["notes"]) == 1 and r["notes"][0]["icon_loaded"])
        self.capture("01-notification.png", surface=True)
        self.hold("live notification in OctoSense shade")
        self.open_reply("reply")
        message = "Hello from OnePlus 6 — 你好 👋"
        self.text(message)
        ready = self.state("unicode-draft", lambda v: self.launcher(v)["reply_editor"].get("send_enabled")
                           and self.launcher(v)["reply_editor"]["length"] == len(message.encode("utf-16-le")) // 2)
        self.capture("02-reply-editor.png")
        self.hold("native reply editor with test text")
        editor = self.launcher(ready)["reply_editor"]
        point = {"x": editor["send_x"], "y": editor["send_y"]}
        self.tap(point)
        self.tap(point)
        self.state("reply-delivered", lambda v: v["notification_flow"]["received"] == 1
                   and v["notification_flow"]["reply"] == message
                   and self.launcher(v)["reply_editor"].get("status") == "Reply sent to the app.")
        self.capture("03-reply-delivered.png")
        self.hold("reply delivered to the fixture app")
        self.request("/flow/back")
        updated = self.renderer("reply-updated-card", lambda r: len(r["notes"]) == 1 and r["notes"][0]["body"] == "Reply received: " + message)
        assert self.launcher(updated)["notification_renderer"]["notes"][0]["id"] == self.launcher(initial)["notification_renderer"]["notes"][0]["id"], "Notification update replaced the visible card"
        self.report["stable_notification_card_verified"] = True
        self.open_reply("stale-draft")
        self.text("This draft must expire")
        self.request("/flow/update")
        self.state("updated-draft-invalidated", lambda v: self.launcher(v)["reply_editor"].get("terminal")
                   and self.launcher(v)["reply_editor"].get("length") == 0
                   and "notification changed" in self.launcher(v)["reply_editor"].get("status", ""))
        self.capture("04-expired-draft.png")
        self.request("/flow/back")
        value = self.reveal("dismiss")
        note = self.launcher(value)["notification_renderer"]["notes"][0]
        self.tap(self.point(value, note["dismiss_bounds"]))
        self.renderer("dismissed-renderer", lambda r: not r["notes"])
        self.state("dismissed-native", lambda v: not v["notification_flow"]["posted"] and v["notification_flow"]["removed"])
        self.capture("05-dismissed.png", surface=True)
        self.request("/flow/post")
        self.open_reply("revoke-draft")
        self.text("Access revocation must clear this draft")
        listener(False)
        self.report["phases"].append("ready_for_revoke")
        self.state("revoked-editor", lambda v: v["notification_flow"]["revoked"] and not v["notification_flow"]["accessible"]
                   and self.launcher(v)["reply_editor"].get("terminal") and self.launcher(v)["reply_editor"].get("length") == 0
                   and v["notification_flow"]["received"] == 1)
        self.capture("06-revoked-draft.png")
        self.request("/flow/back")
        self.renderer("revoked-renderer", lambda r: not r["notes"])
        self.capture("07-access-revoked.png", surface=True)
        self.report["ui_roundtrip_verified"] = True
        self.request("/flow/finish")
        process.wait(timeout=20)

    def close(self):
        if self.base:
            try:
                self.request("/flow/finish")
            except OSError:
                pass
        if self.forward:
            if any("tcp:" + self.forward in line.split() for line in self.command("forward", "--list").splitlines()):
                self.command("forward", "--remove", "tcp:" + self.forward)
            self.forward = None
