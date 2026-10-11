#!/usr/bin/env python3
"""Exercise curved swipes on an assigned Android device and a separate test APK."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time

PACKAGE = 'dev.makepad.octosense.glanceswipe'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', default='adb')
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    prefix = [args.adb, '-s', args.serial]

    def adb(*command, binary=False):
        return subprocess.check_output(prefix+list(command), text=not binary, timeout=30)

    def foreground():
        activity = adb('shell', 'dumpsys', 'activity', 'activities')
        assert any(PACKAGE+'/' in line and ('topResumedActivity=' in line or 'mResumedActivity:' in line)
                   for line in activity.splitlines()), 'Test activity lost foreground; refusing to drive another app'

    def capture(name):
        foreground()
        (out/(name+'.png')).write_bytes(adb('exec-out', 'screencap', '-p', binary=True))

    size = adb('shell', 'wm', 'size')
    sizes = re.findall(r'(?:Physical|Override) size: (\d+)x(\d+)', size)
    width, height = map(int, sizes[-1])
    assert height > width, 'Run this portrait gesture check with the device in portrait'

    def point(x, y):
        return round(width*x/1080), round(height*y/2280)

    def swipe_right():
        foreground()
        x1, y1 = point(250, 1750)
        x2, y2 = point(850, 1750)
        adb('shell', 'input', 'swipe', str(x1), str(y1), str(x2), str(y2), '350')
        time.sleep(.7)

    adb('shell', 'am', 'start', '-W', '-n', PACKAGE+'/.MakepadApp')
    time.sleep(3)
    foreground()
    pid = adb('shell', 'pidof', PACKAGE).strip()
    assert pid.isdecimal(), 'Expected exactly one test process'

    def logs():
        return adb('logcat', '-d', '--pid='+pid)

    capture('home')
    swipe_right()
    capture('overview')
    # A vertical drag cannot scroll this short, fresh-install feed. It must
    # leave Glance in place so the following left swipe can return Home.
    foreground()
    x1, y1 = point(540, 1500)
    x2, y2 = point(540, 1200)
    adb('shell', 'input', 'swipe', str(x1), str(y1), str(x2), str(y2), '350')
    time.sleep(.7)
    capture('after-vertical')
    results = []
    failure = None
    try:
        for y in [600, 900, 1400, 1850]:
            for sign in [-1, 1]:
                foreground()
                before = logs().count('gesture commit Page(Left)')
                positions = [(850, y)] + [(x, y + sign*40) for x in [838, 780, 670, 530, 400, 250]]
                events = []
                for i, (x, yy) in enumerate(positions):
                    px, py = point(x, yy)
                    events.append(f'input motionevent {"DOWN" if i == 0 else "MOVE"} {px} {py}')
                events.append(f'input motionevent UP {px} {py}')
                adb('shell', '\n'.join(events))
                time.sleep(.7)
                foreground()
                passed = logs().count('gesture commit Page(Left)') == before+1
                capture(f'left-{y}-{sign}')
                results.append({'start_y_reference_2280': y, 'vertical_arc_reference': sign*40,
                                'returned_home': passed})
                assert passed, f'Curved left swipe blocked at y={y}, arc={sign*40}'
                swipe_right()
        capture('ready-to-retest')
    except Exception as error:
        failure = str(error)
        raise
    finally:
        # Only gesture diagnostics; the test never reads personal app state.
        diagnostic = '\n'.join(line for line in logs().splitlines() if 'gesture' in line)
        (out/'gestures.log').write_text(diagnostic+'\n')
        screenshots = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.png')}
        status = 'passed' if failure is None else ('interrupted' if 'lost foreground' in failure else 'failed')
        (out/'receipt.json').write_text(json.dumps({'package': PACKAGE, 'physical_mobile': True,
            'status': status, 'error': failure,
            'input': 'Android MotionEvent through adb input', 'screen': [width, height],
            'gestures': results, 'screenshots': screenshots}, indent=2)+'\n')
    print('PASS: eight curved left swipes on the assigned Android device')


if __name__ == '__main__':
    main()
