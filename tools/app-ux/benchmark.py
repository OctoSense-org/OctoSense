#!/usr/bin/env python3
"""Compare native editor-open observations on two release binaries (not GPU FPS)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import time
from capture import AppNative


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--before', type=Path, required=True)
    p.add_argument('--after', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    results = {}
    for name, binary in [('before', args.before), ('after', args.after)]:
        state = out / name
        os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'), OCTOS_APP_CORE_DIR=str(state/'core'))
        a = AppNative(binary.resolve(), ['--test-action', 'phone:ios', '--test-action', 'launch-calendar'], out, name)
        samples = []
        try:
            # First launch also loads the host's icon/style resources. A cold
            # copied baseline may take longer; startup is outside these samples.
            a.wait(lambda: a.find(text='+ Event'), timeout=60)
            time.sleep(1)
            for _ in range(24):
                started = time.perf_counter()
                a.click('+ Event')
                a.wait(lambda: a.find(identifier='e_title', kind='TextInput'))
                samples.append((time.perf_counter() - started) * 1000)
                a.click('Cancel')
                a.wait(lambda: a.find(text='+ Event'))
                time.sleep(.1)
            a.check_logs()
        finally:
            a.close()
        results[name] = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                         'samples_ms': samples, 'p50_ms': statistics.median(samples),
                         'p95_ms': sorted(samples)[22], 'max_ms': max(samples)}
    results['method'] = 'Monotonic wall time from native pointer dispatch wait=1 to observing the editor. Includes HTTP/selector overhead. Hidden phone-size host, isolated state, same machine; other build jobs may run. Not GPU frame latency or a controlled lab benchmark.'
    (out/'receipt.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps(results))


if __name__ == '__main__':
    main()
