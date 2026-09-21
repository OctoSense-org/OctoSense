#!/usr/bin/env python3
"""Exercise the native reply editor in an owned Home validation instance.

Only synthetic text enters the fixture; it sends no notification or message.
Input uses the editor's Android InputConnection and app-local touch dispatch.
Captures use this app's Window, excluding the system keyboard and display.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]

    def command(*parts):
        return subprocess.check_output(adb + list(parts), text=True, timeout=15).strip()

    def remotes():
        return re.findall(r"--remote pid=(\d+) port=(\d+) token=([a-f0-9-]+)",
                          command("logcat", "-d", "-s", "OctoSenseValidation:I", "*:S"))

    previous = set(remotes())
    report = {"serial": args.serial, "result": "fail", "states": {}, "captures": [],
              "external_message_sent": False, "notification_listener_tested": False,
              "bridge_roundtrip_tested": False, "capture_scope": "owned app Window only"}
    base = forward = None
    closed = False
    with (args.output / "instrumentation.log").open("w") as log:
        runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "reply_ui",
            "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"], stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                found = [item for item in remotes() if item not in previous]
                if found:
                    pid, port, token = found[-1]
                    report["owned_pid"] = int(pid)
                    forward = command("forward", "tcp:0", "tcp:" + port)
                    base = "http://127.0.0.1:" + forward + "/" + token
                    break
                if runner.poll() is not None:
                    raise RuntimeError("Instrumentation ended before remote startup")
                time.sleep(0.5)
            assert base, "Validation remote unavailable"

            def request(route):
                with urllib.request.urlopen(base + route, timeout=10) as response:
                    return response.read()

            def state(name, predicate, timeout=18):
                deadline = time.monotonic() + timeout
                value = None
                while time.monotonic() < deadline:
                    value = json.loads(request("/status"))
                    assert value["pid"] == report["owned_pid"], "Owned PID changed"
                    reply = value["reply_fixture"]
                    if predicate(reply):
                        report["states"][name] = reply
                        return reply
                    time.sleep(0.15)
                report["states"][name + "-timeout"] = value
                raise AssertionError("State timeout: " + name)

            def text(value):
                request("/reply/text?" + urllib.parse.urlencode({"value": value}))

            def tap_send(value):
                request("/tap?" + urllib.parse.urlencode({"x": value["send_x"], "y": value["send_y"]}))

            def capture(name):
                data = request("/g")
                assert data.startswith(b"\x89PNG\r\n\x1a\n")
                (args.output / name).write_bytes(data)
                report["captures"].append(name)

            state("fixture", lambda r: r.get("available"))
            request("/reply/open")
            state("empty", lambda r: r["visible"] and not r["send_enabled"])
            state("native-ime", lambda r: r["editor_focused"] and r.get("ime_visible", False))
            text(" \n")
            state("whitespace", lambda r: r["length"] == 2 and not r["send_enabled"])
            request("/reply/clear")
            message = "Native reply: 你好 👋\nsecond line"
            text(message)
            ready = state("unicode-draft", lambda r: r["send_enabled"] and r["length"] == len(message.encode("utf-16-le")) // 2)
            capture("reply-editor.png")
            tap_send(ready)
            pending = state("submitted", lambda r: r["pending"] and r["submissions"] == 1)
            assert pending["fixture_text"] == message
            tap_send(pending)
            time.sleep(0.2)
            request("/reply/result?status=0")
            state("accepted", lambda r: r["pending"] and r["submissions"] == 1 and not r["send_enabled"])
            request("/reply/result?status=1")
            state("completed", lambda r: r["terminal"] and r["length"] == 0 and r["status"] == "Reply sent to the app.")
            capture("reply-completed.png")

            request("/reply/open")
            text("New draft")
            request("/reply/result?status=5")  # Late result for the previous token.
            state("old-result-ignored", lambda r: r["length"] == 9 and r["send_enabled"] and not r["terminal"])
            request("/reply/invalidate")
            state("expired", lambda r: r["terminal"] and r["length"] == 0 and not r["send_enabled"])
            request("/reply/open")
            text("Cancel this draft")
            request("/reply/close")
            state("cancelled", lambda r: not r["visible"] and r["submissions"] == 1)

            request("/reply/open")
            text("Queue pressure")
            request("/reply/busy")
            ready = state("busy-draft", lambda r: r["send_enabled"])
            tap_send(ready)
            state("queue-full", lambda r: r["status"] == "Home is busy. Try again." and r["send_enabled"] and r["submissions"] == 1)

            request("/reply/open")
            text("Unknown outcome")
            tap_send(state("timeout-draft", lambda r: r["send_enabled"]))
            state("timeout-pending", lambda r: r["pending"] and r["submissions"] == 2)
            uncertain = state("timeout", lambda r: r["terminal"] and "Check the conversation" in r["status"], timeout=18)
            assert not uncertain["send_enabled"] and uncertain["submissions"] == 2
            capture("reply-uncertain.png")
            request("/reply/result?status=1")
            state("late-completion", lambda r: r["length"] == 0 and r["status"] == "Reply sent to the app.")

            request("/reply/open")
            text("x" * 2001)
            state("bounded-text", lambda r: r["length"] == 2000)
            request("/reply/disconnect")
            state("disconnected-draft", lambda r: r["terminal"] and r["length"] == 0 and not r["send_enabled"])
            request("/reply/open")
            text("Disconnected while sending")
            tap_send(state("disconnect-draft", lambda r: r["send_enabled"]))
            state("disconnect-pending", lambda r: r["pending"] and r["submissions"] == 3)
            request("/reply/disconnect")
            state("disconnected-send", lambda r: r["terminal"] and "Check the conversation" in r["status"] and not r["send_enabled"])
            request("/reply/close")
            request("/gq")
            closed = True
            runner.wait(timeout=20)
            log.flush()
            output = (args.output / "instrumentation.log").read_text()
            assert "result=pass" in output and "INSTRUMENTATION_CODE: -1" in output, output
            report["result"] = "pass"
        except Exception as exc:
            report["error"] = str(exc)
            raise
        finally:
            if base and not closed:
                try:
                    urllib.request.urlopen(base + "/quit", timeout=8).read()
                    runner.wait(timeout=15)
                except (OSError, subprocess.TimeoutExpired):
                    pass
            if forward:
                command("forward", "--remove", "tcp:" + forward)
            if "owned_pid" in report:
                deadline = time.monotonic() + 10
                while True:
                    process = subprocess.run(adb + ["shell", "ps", "-p", str(report["owned_pid"]), "-o", "PID="], capture_output=True, text=True, timeout=10)
                    report["owned_pid_exited"] = str(report["owned_pid"]) not in process.stdout.split()
                    if report["owned_pid_exited"] or time.monotonic() >= deadline:
                        break
                    time.sleep(0.1)
            report["instrumentation_cleanup_pending"] = runner.poll() is None
            (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"result": report["result"], "output": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()
