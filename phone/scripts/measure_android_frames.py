#!/usr/bin/env python3
"""Measure one Android gesture using SurfaceFlinger's presentation timestamps.

Run one action per sample, with the frame monitor off. The JSON retains every
raw timestamp, the legacy <250 ms statistics, and an activity span that keeps
long stalls BETWEEN busy frames. Activity boundaries are inferred from presents,
not input events: this measures frame pacing, not touch-to-display latency.

Example:
  python3 scripts/measure_android_frames.py --adb /path/to/adb --serial SERIAL \
    --command 'input swipe 300 130 300 1300 200' --output shade-open.json
"""
import argparse
from bisect import bisect_right
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import re
import shlex
import subprocess
import time


PENDING = 2**63 - 1


def parse_latency(text):
    rows = set()
    for line in text.splitlines():
        fields = line.split()
        if len(fields) != 3 or not all(v.isdigit() for v in fields):
            continue
        row = tuple(map(int, fields))
        if all(0 < v < PENDING for v in row):
            rows.add(row)
    return rows


def layer_names(text):
    for line in text.splitlines():
        name = line.strip()
        # Android 15 dumps requested-layer wrappers; Android 11 prints names.
        # The leading hex token is part of the layer name, not a wrapper ID.
        match = re.match(r"RequestedLayerState\{(.+?)(?: parentId=|\}$)", name)
        if match:
            name = match[1]
        if name:
            yield name


def quantiles(intervals, refresh_ms=1000 / 60):
    if not intervals:
        return None
    ordered = sorted(intervals)
    q = lambda p: ordered[min(len(ordered) - 1, int(p * len(ordered)))]
    refreshes = Counter(max(1, int(v / refresh_ms + .5)) for v in intervals)
    return {
        "intervals": len(ordered), "fps": 1000 * len(ordered) / sum(ordered),
        "p50_ms": q(.5), "p95_ms": q(.95), "max_ms": ordered[-1],
        "over_34_ms": sum(v > 34 for v in ordered),
        "over_250_ms": sum(v >= 250 for v in ordered),
        "refresh_multiple_counts": dict(sorted(refreshes.items())),
        "missed_refreshes": sum((n - 1) * count for n, count in refreshes.items()),
    }


def summarize(rows, refresh_ms=1000 / 60):
    # Deduplicate presents even if another field changed between polls.
    presents = sorted({row[1] for row in rows})
    intervals = [(b - a) / 1e6 for a, b in zip(presents, presents[1:])]
    busy = [i for i, value in enumerate(intervals) if value < 250]
    # Trim only leading/trailing idle. Never remove an interior stall.
    active = intervals[busy[0]:busy[-1] + 1] if len(busy) >= 3 else []
    return {
        "presents": len(presents),
        "all_intervals": quantiles(intervals, refresh_ms),
        "activity_span": quantiles(active, refresh_ms),
        "legacy_burst": quantiles([v for v in intervals if v < 250], refresh_ms),
        "activity_boundaries": "inferred; leading/trailing gaps >=250 ms trimmed",
        "status": "measured" if active else "insufficient activity; not a passing result",
    }


def summarize_marked(rows, markers, refresh_ms=1000 / 60, input_ns=None):
    """Join buffer submission time to the last recorded scene on that clock."""
    markers = sorted(set(markers))
    times = [m[0] for m in markers]
    spans = []
    span = -1
    was_active = False
    for _, active in markers:
        if active and not was_active:
            span += 1
        spans.append(span if active else None)
        was_active = active
    groups = {}
    for row in sorted(rows, key=lambda r: r[1]):
        if input_ns is not None and row[0] < input_ns:
            continue
        index = bisect_right(times, row[0]) - 1
        if index >= 0 and spans[index] is not None:
            groups.setdefault(spans[index], set()).add(row[1])
    intervals = []
    counts = []
    for values in groups.values():
        presents = sorted(values)
        counts.append(len(presents))
        intervals.extend((b - a) / 1e6 for a, b in zip(presents, presents[1:]))
    return {"statistics": quantiles(intervals, refresh_ms), "span_present_counts": counts,
            "method": "submission timestamp joined to scene markers; gaps within an active span retained"}


def first_state_response(rows, input_ns, scenes):
    """Bound a response using visible shell state; pixels are not read back."""
    before = [state for ns, state in scenes if ns < input_ns]
    if not before:
        return None
    prior = before[-1]
    for ns, state in scenes:
        if ns < input_ns:
            continue
        changed = state[0] != prior[0] or any(abs(a - b) >= .003 for a, b in zip(state[1:], prior[1:]))
        if changed:
            presents = [row[1] for row in rows if row[0] >= ns]
            if presents:
                return {"input_to_present_ms": (min(presents) - input_ns) / 1e6,
                        "input_ns": input_ns, "state_change_ns": ns,
                        "method": "first changed shell-state scene, then first actual present; visible pixels not read back"}
            break
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", required=True)
    parser.add_argument("--package", default="dev.makepad.octosense")
    parser.add_argument("--layer", help="Exact SurfaceFlinger layer name")
    parser.add_argument("--layer-match", help="Regex for a surface recreated during the action (e.g. native NotificationShade)")
    parser.add_argument("--frame-markers", action="store_true", help="Read OctoSense phone.frames trace (launch with --es makepad.TRACE phone.frames)")
    parser.add_argument("--input-markers", action="store_true", help="Require phone.input timestamps and exclude frames submitted before the new touch")
    parser.add_argument("--command", required=True, help="One gesture, executed by Android sh")
    parser.add_argument("--seconds", type=float, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error("--seconds must be positive")
    if args.input_markers and not args.frame_markers:
        parser.error("--input-markers requires --frame-markers")

    def adb(*command):
        result = subprocess.run([args.adb, "-s", args.serial, *command],
                                capture_output=True, text=True, timeout=15, check=True)
        return result.stdout

    if adb("get-state").strip() != "device":
        raise RuntimeError("Phone is not ready")
    pid = adb("shell", "pidof " + shlex.quote(args.package)).strip() if args.frame_markers else None
    trace_floor_ns = None
    if args.input_markers:
        last = adb("shell", f"logcat -d --pid={pid} -t 1000 -s Makepad:I | "
                   "sed -n 's/.*\\[phone.frames\\] //p' | tail -n 1")
        match = re.search(r"ns=(\d+)", last)
        if not match:
            raise RuntimeError("No pre-action phone.frames marker; discard this measurement")
        trace_floor_ns = int(match[1])
    layer = args.layer
    if not layer and not args.layer_match:
        candidates = [name for name in layer_names(adb("shell", "dumpsys SurfaceFlinger --list"))
                      if args.package in name and "SurfaceView" in name
                      and not name.startswith("Background for")]
        candidates.sort(key=lambda name: ("(BLAST)" not in name, name))
        if not candidates:
            raise RuntimeError("No app SurfaceView found; launch the app first or supply --layer")
        layer = candidates[0]

    layers_seen = set()
    refresh_periods = set()
    def sample():
        selected = layer
        if args.layer_match:
            candidates = [name for name in layer_names(adb("shell", "dumpsys SurfaceFlinger --list"))
                          if re.search(args.layer_match, name)]
            if not candidates:
                return set()
            selected = max(candidates, key=lambda name: int(name.rsplit('#', 1)[-1]))
        layers_seen.add(selected)
        latency = adb("shell", "dumpsys SurfaceFlinger --latency " + shlex.quote(selected))
        first = latency.splitlines()[0] if latency.splitlines() else ""
        if first.isdigit() and int(first) > 0:
            refresh_periods.add(int(first))
        return parse_latency(latency)

    before = sample()
    rows = set()
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as worker:
        action = worker.submit(adb, "shell", args.command)
        while time.monotonic() - started < args.seconds or not action.done():
            if time.monotonic() - started > args.seconds + 20:
                raise RuntimeError("Gesture did not finish; discard this measurement")
            rows.update(sample() - before)
            time.sleep(.15)
        command_output = action.result()
    rows.update(sample() - before)
    if not rows:
        raise RuntimeError("No presentation timestamps; layer may have changed. Discard this measurement")
    if len(refresh_periods) != 1:
        raise RuntimeError("Missing or changing display refresh period; discard this measurement")
    refresh_ns = next(iter(refresh_periods))
    result = {
        "serial": args.serial, "layer": layer, "layers_seen": sorted(layers_seen), "command": args.command,
        "elapsed_s": time.monotonic() - started,
        "refresh_period_ns": refresh_ns,
        "summary": summarize(rows, refresh_ns / 1e6), "command_output": command_output,
        "raw_columns": ["desired_present_ns", "actual_present_ns", "frame_ready_ns"],
        "raw": sorted(rows, key=lambda row: row[1]),
    }
    if args.frame_markers:
        if not pid or adb("shell", "pidof " + shlex.quote(args.package)).strip() != pid:
            raise RuntimeError("App process changed; discard the sample")
        # Filter on-device so long warm-run sessions do not transfer the whole
        # trace history over USB. Include the last idle scene before the run.
        earliest = trace_floor_ns if trace_floor_ns is not None else min(row[0] for row in rows) - 2_000_000_000
        trace = adb("shell", f"logcat -d --pid={pid} -t 4000 -s Makepad:I | "
                    "sed -n -e 's/.*\\[phone.frames\\] /frames /p' "
                    "-e 's/.*\\[phone.input\\] /input /p' | "
                    f"awk 'substr($2,4)+0 >= {earliest} "
                    "&& ($1 == \"frames\" || $3 == \"phase=Down\" || $3 == \"phase=Up\") "
                    "{print $1, $2, $3}'")
        markers = [(int(ns), int(active)) for ns, active in re.findall(r"frames ns=(\d+) active=([01])", trace)]
        if not markers:
            raise RuntimeError("No phone.frames markers; relaunch with the trace extra")
        result["frame_markers"] = markers
        input_ns = None
        if args.input_markers:
            downs = [int(ns) for ns in re.findall(r"input ns=(\d+) phase=Down", trace)]
            if not downs:
                raise RuntimeError("No phone.input Down marker for the action; discard this measurement")
            input_ns = downs[0]
            result["input_ns"] = input_ns
            result["summary"]["input_boundary"] = "shell-received touch Down; frames submitted before it excluded"
            later = [row[1] for row in rows if row[0] >= input_ns]
            if later:
                result["summary"]["first_post_input_present_ms"] = (min(later) - input_ns) / 1e6
            detailed = adb("shell", f"logcat -d --pid={pid} -t 4000 -s Makepad:I | "
                           "sed -n 's/.*\\[phone.frames\\] //p' | "
                           f"awk 'substr($1,4)+0 >= {trace_floor_ns} {{print; if (++n >= 80) exit}}'")
            scenes = []
            for line in detailed.splitlines():
                match = re.search(r"ns=(\d+) active=[01] screen=(\w+) shade=([\d.]+) "
                                  r"overview=([\d.]+) openness=([\d.]+) page=([-\d.]+) pages=([-\d.]+)", line)
                if match:
                    scenes.append((int(match[1]), (match[2], *(float(match[i]) for i in range(3, 8)))))
            result["summary"]["first_state_response"] = first_state_response(rows, input_ns, scenes)
        result["summary"]["marked_activity"] = summarize_marked(rows, markers, refresh_ns / 1e6, input_ns)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["summary"], indent=2))


if __name__ == "__main__":
    main()
