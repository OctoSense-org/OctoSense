#!/usr/bin/env python3
"""Exercise the ROM's notch-expanded pull-down region over Home and another app.

Retains window identities and gesture coordinates, never notification contents.
Run with --expect-stock-gap only against the retained pre-fix APK.
"""
import argparse
import importlib
import json
from pathlib import Path
import re
import time

Phone = importlib.import_module('run-global-shade-validation').Phone


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-stock-gap', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    phone = Phone(args.adb, args.serial)
    report = {'result': 'fail', 'expect_stock_gap': args.expect_stock_gap, 'checks': []}
    rotation = {}
    try:
        phone.wake_home()
        display = phone.command('shell', 'dumpsys window displays')
        threshold = int(re.search(r'mSwipeStartThreshold=Rect\(\d+, (\d+) -', display)[1])
        inputs = phone.command('shell', 'dumpsys input')
        bar = int(re.search(r'name=[^\n]* StatusBar,[^\n]*frame=\[0,0\]\[\d+,(\d+)\]', inputs)[1])
        assert 0 < bar < threshold < 200, 'This regression requires the OnePlus notch boundary'
        report.update(status_bar_height_px=bar, android_swipe_start_threshold_px=threshold)
        for app in ['bridge', 'home']:
            for x in [70, 1010]:
                for y in [1, bar + 1, threshold]:
                    phone.wake_home()
                    if app == 'bridge':
                        phone.app()
                    before = phone.focus()
                    phone.command('shell', 'input', 'swipe', str(x), str(y), str(x), '1250', '450')
                    expected = 'NotificationShade' if args.expect_stock_gap and y > bar else 'OctoSense Global Shade'
                    after = phone.wait_focus(expected)
                    if expected == 'OctoSense Global Shade':
                        time.sleep(.25)
                        phone.command('shell', 'input', 'keyevent', '4')
                        returned = phone.wait_focus(before.split(' u0 ', 1)[1].rstrip('}'))
                        assert returned == before, 'Close changed the underlying app window'
                    report['checks'].append({'app': app, 'x': x, 'y': y, 'window': expected, 'passed': True})
        # An ordinary app swipe immediately below the system boundary stays in
        # that app; widening to the whole top portion would steal its controls.
        phone.wake_home()
        phone.app()
        before = phone.focus()
        phone.command('shell', 'input', 'swipe', '1010', str(threshold + 1), '1010', '1250', '450')
        time.sleep(.6)
        assert phone.focus() == before, 'App gesture below the system boundary was intercepted'
        report['checks'].append({'app': 'bridge', 'y': threshold + 1, 'ordinary_app_gesture': True, 'passed': True})
        if not args.expect_stock_gap:
            rotation = {key: phone.command('shell', 'settings', 'get', 'system', key)
                        for key in ['accelerometer_rotation', 'user_rotation']}
            phone.command('shell', 'settings', 'put', 'system', 'accelerometer_rotation', '0')
            for orientation in ['1', '3']:
                phone.command('shell', 'settings', 'put', 'system', 'user_rotation', orientation)
                time.sleep(1)
                phone.app()
                display = phone.command('shell', 'dumpsys window displays')
                edge = int(re.search(r'mSwipeStartThreshold=Rect\(\d+, (\d+) -', display)[1])
                before = phone.focus()
                for x in [140, 2140]:
                    phone.command('shell', 'input', 'swipe', str(x), str(edge), str(x), '650', '450')
                    phone.wait_focus('OctoSense Global Shade')
                    time.sleep(.25)
                    phone.command('shell', 'input', 'keyevent', '4')
                    phone.wait_focus(before.split(' u0 ', 1)[1].rstrip('}'))
                    report['checks'].append({'rotation': orientation, 'x': x, 'y': edge, 'passed': True})
        report['result'] = 'pass'
    except Exception as failure:
        report['failure'] = str(failure)
    finally:
        try:
            for key in ['user_rotation', 'accelerometer_rotation']:
                if key in rotation:
                    if rotation[key] == 'null':
                        phone.command('shell', 'settings', 'delete', 'system', key)
                    else:
                        phone.command('shell', 'settings', 'put', 'system', key, rotation[key])
            if rotation:
                assert all(phone.command('shell', 'settings', 'get', 'system', key) == value
                           for key, value in rotation.items()), 'Original rotation settings were not restored'
                report['rotation_restored'] = True
                time.sleep(1)
            phone.wake_home()
        except Exception as failure:
            report['result'] = 'fail'
            report['restore_error'] = str(failure)
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if report['result'] == 'pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
