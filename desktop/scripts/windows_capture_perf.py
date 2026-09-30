#!/usr/bin/env python3
"""Measure Windows remote capture latency and UI responsiveness in a hidden shell.

Run with an absolute --binary path; use different --output directories to compare
builds. Test data is isolated beneath --output. No user's running shell is touched.
"""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import socket
import statistics
import subprocess
import time
from urllib.parse import urlencode
from urllib.request import urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('This probe requires Windows')
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = os.environ.copy()
    env.update(MAKEPAD_HIDE_WINDOWS='1', MAKEPAD_REMOTE=str(port),
               MAKEPAD_TRACE='startup,frames,remote.grab', MAKEPAD_DEVTOOLS='0',
               OCTOSENSE_HOME=str(output / 'home'),
               OCTOSENSE_APP_DATA=str(output / 'home/apps'),
               RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-msvc')
    env.pop('OCTOS_APP_CORE_BIN', None)

    def get(route, **params):
        url = f'http://127.0.0.1:{port}/{route}'
        if params:
            url += '?' + urlencode(params)
        with urlopen(url, timeout=30) as response:
            result = json.load(response)
        if isinstance(result, dict) and 'err' in result:
            raise RuntimeError(result['err'])
        return result

    result = {'binary': str(args.binary.resolve()), 'captures': []}
    with (output / 'stdout.log').open('w', encoding='utf-8') as stdout, \
            (output / 'stderr.log').open('w', encoding='utf-8') as stderr:
        process = subprocess.Popen([str(args.binary.resolve())], cwd=root,
                                   env=env, stdout=stdout, stderr=stderr,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        result['pid'] = process.pid
        try:
            deadline = time.monotonic() + 40
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f'Shell exited: {process.returncode}')
                try:
                    state = get('s')
                    if state.get('w'):
                        result['window'] = state
                        break
                except OSError:
                    pass
                time.sleep(.05)
            else:
                raise TimeoutError('Shell did not expose a window')
            # Exclude shader compilation and first-use asset work from the trial.
            get('g', scale=.25)
            time.sleep(2)
            for scale in (1.0, .5, .25):
                for trial in range(3):
                    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as worker:
                        started = time.perf_counter()
                        capture = worker.submit(get, 'g', scale=scale)
                        time.sleep(.075)
                        input_started = time.perf_counter()
                        get('k', k='down', c='Escape', wait=1)
                        input_ms = (time.perf_counter() - input_started) * 1000
                        # Read the completed capture before doing any more input.
                        grab = capture.result()
                        total_ms = (time.perf_counter() - started) * 1000
                        get('k', k='up', c='Escape')
                    image = output / f'grab-{scale}-{trial}.png'
                    shutil.copyfile(grab['png'], image)
                    result['captures'].append(dict(scale=scale, trial=trial,
                                                   input_ms=input_ms, total_ms=total_ms,
                                                   grab=grab, image=str(image)))
                    time.sleep(.2)
            # A scheduled burst exercises ownership of multiple queued raw frames.
            burst = get('gseq', n=3, every_ms=100, scale=.25)
            assert len(burst['png']) == 3, burst
            result['burst'] = burst
            for scale in (1.0, .5, .25):
                samples = [s for s in result['captures'] if s['scale'] == scale]
                result.setdefault('summary', []).append({
                    'scale': scale,
                    'median_capture_ms': statistics.median(s['grab']['capture_ms'] for s in samples),
                    'median_encode_ms': statistics.median(s['grab']['encode_ms'] for s in samples),
                    'median_total_ms': statistics.median(s['total_ms'] for s in samples),
                    'median_input_ms_during_capture': statistics.median(s['input_ms'] for s in samples),
                    'capture_kinds': sorted(set(s['grab']['capture_kind'] for s in samples)),
                })
        finally:
            try:
                result['quit'] = get('quit')
            except Exception as error:
                result['quit_error'] = str(error)
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
            result['exit_code'] = process.returncode
            (output / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result['summary'], indent=2), flush=True)


if __name__ == '__main__':
    main()
