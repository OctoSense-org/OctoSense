"""Correlate drawer scene positions with injected input and EGL display times.

Input: scenario JSON with before/frames/injection, and its sibling .log with
phone.frames + android.present. Times must share Android's monotonic clock.
This is software-injected content age, not hardware touch-to-photon latency.
Never label a run a latency pass when its input cadence check fails.
"""
import argparse
import bisect
import json
import math
from pathlib import Path
import re


def injections(text):
    rows = []
    for line in text.splitlines():
        if line.count(",") == 3 and not line.startswith("#"):
            ns, action, x, y = line.split(",")
            if int(action) in (0, 2):
                rows.append((int(ns), float(x), float(y)))
    if any(ns < 0 or not math.isfinite(x) or not math.isfinite(y) for ns, x, y in rows):
        raise ValueError("Input timestamps and coordinates must be valid and finite")
    if len(rows) < 2 or any(b[0] <= a[0] for a, b in zip(rows, rows[1:])):
        raise ValueError("Need increasing actual input timestamps")
    return rows


def percentile(values, fraction):
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def stats(values):
    return {key: round(percentile(values, q), 2)
            for key, q in (("p50", .5), ("p95", .95), ("max", 1))} if values else None


def summarize(scenario, presents, dpi, max_input_gap_ms):
    moves = injections(scenario["injection"])
    gaps = [(b[0] - a[0]) / 1e6 for a, b in zip(moves, moves[1:])]
    xs = [moves[0][2] - move[2] for move in moves]
    if any(b < a for a, b in zip(xs, xs[1:])):
        raise ValueError("Expected one downward-list drag with monotonic positions")
    frames = scenario["frames"]
    ns = [frame["ns"] for frame in frames]
    if any(b <= a for a, b in zip(ns, ns[1:])):
        raise ValueError("Scene timestamps must increase")
    samples, seen = [], set()
    for row in presents:
        j = bisect.bisect_right(ns, row["submitted_ns"]) - 1
        if row["present_ns"] <= 0 or not 0 <= j < len(frames) - 1:
            continue
        frame = frames[j]
        position = (frame["search_scroll"] - scenario["before"]["search_scroll"]) * dpi
        if not 100 < position < min(1100, xs[-1]):
            continue
        k = bisect.bisect_right(xs, position) - 1
        if not 0 <= k < len(xs) - 1:
            continue
        if j in seen:
            raise ValueError("More than one submitted buffer matched a scene")
        seen.add(j)
        fraction = (position - xs[k]) / (xs[k + 1] - xs[k])
        estimated_input_ns = moves[k][0] + (moves[k + 1][0] - moves[k][0]) * fraction
        samples.append({
            "frame_id": row["id"],
            "content_age_ms": (row["present_ns"] - estimated_input_ns) / 1e6,
            "input_to_scene_ms": (frame["ns"] - estimated_input_ns) / 1e6,
            "source_gap_ms": gaps[k],
            "source_gap_exceeded": gaps[k] > max_input_gap_ms,
        })
    if not samples:
        raise ValueError("No scene/input/EGL matches; check trace topics and DPI")
    # Retain EVERY matched frame and its untrimmed maximum. A slow injector
    # invalidates the run's attribution; it never makes an outlier disappear.
    return {
        "scenario": scenario["name"],
        "injector_cpu_mask": scenario.get("injector_cpu_mask"),
        "matched_frames": len(samples),
        "input_cadence_valid": max(gaps) <= max_input_gap_ms,
        "input_gap_ms": stats(gaps),
        "max_input_gap_allowed_ms": max_input_gap_ms,
        "observed_content_age_ms": stats([s["content_age_ms"] for s in samples]),
        "worst_samples": sorted(samples, key=lambda s: s["content_age_ms"], reverse=True)[:3],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", nargs="+", type=Path)
    parser.add_argument("--dpi", type=float, required=True, help="Device pixels per layout point")
    parser.add_argument("--max-input-gap-ms", type=float, default=12,
                        help="Cadence gate (default 12 ms for an 8 ms injector)")
    args = parser.parse_args()
    if not math.isfinite(args.dpi) or args.dpi <= 0 or not math.isfinite(args.max_input_gap_ms) or args.max_input_gap_ms <= 0:
        parser.error("DPI and input-gap limit must be positive and finite")
    results = []
    for path in args.captures:
        presents = [{k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", line)}
                    for line in path.with_suffix(".log").read_text().splitlines()
                    if "[android.present]" in line]
        for scenario in json.loads(path.read_text()):
            results.append({"file": path.name, **summarize(scenario, presents, args.dpi, args.max_input_gap_ms)})
    print(json.dumps(results, indent=2))
    return 0 if results and all(r["input_cadence_valid"] for r in results) else 2


if __name__ == "__main__":
    raise SystemExit(main())
