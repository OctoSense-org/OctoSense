#!/usr/bin/env python3
"""Real input → saved event → detail → Glance → process restart, with timings."""
import argparse
import json
import os
from pathlib import Path
import statistics
import time
from urllib.error import HTTPError
from capture import AppNative, ROOT


class CalendarNative(AppNative):
    def call(self, route, **query):
        # Hidden desktop windows can reject an immediate GPU present after
        # input has already been applied. Do not resend that input: observe
        # the next widget state below, and capture actual frames at each
        # checkpoint. Timing is input-to-observed-state, never GPU latency.
        input_event = route in ('click', 'm', 'k', 't')
        if input_event:
            query.pop('wait', None)
        result = super().call(route, **query)
        if input_event:
            # Wait on a separate frame request so widget geometry reflects
            # the input before the next reachability/scroll calculation.
            for attempt in range(15):
                try:
                    super().call('g')
                    break
                except HTTPError as error:
                    if error.code != 404 or attempt == 14:
                        raise
                    time.sleep(.1)
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/octosense")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    os.environ.update(OCTOSENSE_HOME=str(state / "home"), OCTOS_APP_CORE_DIR=str(state / "core"),
                      OCTOSENSE_APP_DATA=str(state / "apps"), OCTOSENSE_LLM_VAULT="file")
    actions = [] if args.desktop else ["--test-action", "phone:ios"]
    actions += ["--test-action", "launch-calendar"]
    title = "Design review · 设计评审"
    samples = []
    checks = []
    for attempt in range(2):
        native_type = CalendarNative if args.desktop else AppNative
        native = native_type(args.binary.resolve(), actions, out, "calendar-" + str(attempt))
        try:
            native.label("Calendar")
            native.wait(lambda: native.find(text="+ Event"))
            native.wait(lambda: (native.find(identifier="day_title", kind="Label") or {}).get("t"))
            time.sleep(.8)  # Wait for the shell opening transition before pointer input.
            if args.desktop:
                # Activate the hosted window before measuring its app controls.
                # Use the non-interactive heading, not an app action, for setup.
                native.click_row(native.label("Calendar"))
            if attempt == 0:
                # A bounded, repeatable interaction; each reply waits for drawing.
                for cycle in range(12):
                    print("editor cycle", cycle, flush=True)
                    start = time.perf_counter()
                    native.click("+ Event")
                    native.wait(lambda: native.find(identifier="e_title", kind="TextInput"))
                    samples.append((time.perf_counter() - start) * 1000)
                    native.click("Cancel")
                    native.wait(lambda: native.find(text="+ Event"))
                    time.sleep(.1)
                native.click("+ Event")
                native.field("e_title", title)
                if not args.desktop:
                    native.call("k", k="press", c="Escape", wait=1)
                    time.sleep(.3)
                native.field("e_place", "Studio · 第二会议室")
                if not args.desktop:
                    native.call("k", k="press", c="Escape", wait=1)
                    time.sleep(.3)
                native.field("e_notes", "Review desktop and phone layouts.\nCheck keyboard, contrast, and saved state.")
                if not args.desktop:
                    native.call("k", k="press", c="Escape", wait=1)
                    time.sleep(.3)
                native.click("Save")
                native.wait(lambda: native.find(text="+ Event"))
                native.reachable(text=title, kind="Label")
                checks.append("Created an event with Unicode title and multiline notes through native inputs")
            else:
                native.reachable(text=title, kind="Label")
                checks.append("Saved event restored after complete process restart")
            native.click("View event  ›")
            native.wait(lambda: native.find(text="Edit"))
            native.label("Studio · 第二会议室")
            native.reachable(identifier="event_notes", kind="Label")
            native.label("Check keyboard, contrast, and saved state.")
            if attempt == 0:
                native.click("Show in Glance")
                native.label("Event shown in Glance.")
                checks.append("Same saved event published to Glance through its registered host service")
                native.capture("calendar-published")
            native.capture("calendar-result-" + str(attempt))
            native.check_logs()
        except Exception:
            (out / "failed-actions.json").write_text(json.dumps(native.actions, indent=2))
            native.capture("failed-state")
            raise
        finally:
            native.close()
    # Read the authoritative store, not the screenshot or assistant prose.
    stores = list(state.rglob("events.json"))
    assert len(stores) == 1, stores
    saved = json.loads(stores[0].read_text())
    assert title in json.dumps(saved, ensure_ascii=False)
    ordered = sorted(samples)
    receipt = {"checks": checks, "physical_mobile": False, "release_build": True,
               "timing_method": "Python monotonic wall time: select + Event, observe editor; includes HTTP and selector overhead, not GPU frame duration",
               "immediate_input_frame_wait": not args.desktop,
               "separate_frame_capture_after_input": args.desktop,
               "samples_ms": samples, "p50_ms": statistics.median(samples),
               "p95_ms": ordered[min(len(ordered)-1, int(len(ordered)*.95))], "max_ms": max(samples),
               "persisted_store": str(stores[0].relative_to(out))}
    (out / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(receipt, ensure_ascii=False))


if __name__ == "__main__":
    main()
