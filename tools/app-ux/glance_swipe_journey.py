#!/usr/bin/env python3
"""Check curved Glance paging gestures in an isolated native phone preview."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import time
from capture import AppNative, ROOT


def drag(native, points):
    native.call('m', k='down', x=points[0][0], y=points[0][1], wait=1)
    for x, y in points[1:]:
        native.call('m', k='move', x=x, y=y, wait=1)
        time.sleep(.025)
    native.call('m', k='up', x=points[-1][0], y=points[-1][1], wait=1)
    time.sleep(.5)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out/'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                      OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
                      OCTOSENSE_LLM_VAULT='file')
    binary = args.binary.resolve()
    native = AppNative(binary, ['--test-action', 'phone:ios', '--test-action', 'page:-1'], out, 'swipe')
    results = []
    try:
        native.wait(lambda: len(native.rows()) > 2, timeout=45)
        time.sleep(2)
        native.capture('overview')
        # A vertical drag on a feed that fits should leave this page in place,
        # including the desktop preview's legacy navigation path. Subsequent
        # left swipes must still start on Glance, not the App Library.
        drag(native, [(200, 600), (200, 575), (200, 550), (200, 500)])
        native.capture('after-vertical')
        for y in [170, 300, 500, 680]:
            for sign in [-1, 1]:
                name = f'left-{y}-{sign}'
                before = native.log_path.read_text().count('gesture commit Page(Left)')
                points = [(300, y)] + [(300 + dx, y + sign*14) for dx in [-4, -24, -80, -160, -210]]
                drag(native, points)
                passed = native.log_path.read_text().count('gesture commit Page(Left)') == before + 1
                native.capture(name)
                results.append({'start_y': y, 'vertical_arc': sign*14, 'returned_home': passed})
                assert passed, f'Curved left swipe blocked at y={y}, arc={sign*14}'
                right = native.log_path.read_text().count('gesture commit Page(Right)')
                drag(native, [(85, 650), (110, 650), (170, 650), (230, 650), (330, 650)])
                assert native.log_path.read_text().count('gesture commit Page(Right)') == right + 1
        native.check_logs()
    finally:
        (out/'receipt.json').write_text(json.dumps({
            'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
            'input': 'native mouse events in phone preview', 'physical_mobile': False,
            'gestures': results,
        }, indent=2)+'\n')
        native.close()
    print('PASS: curved left swipes at four heights, both initial vertical directions')


if __name__ == '__main__':
    main()
