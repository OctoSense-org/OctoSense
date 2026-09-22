#!/usr/bin/env python3
"""Exercise global panel controls, task switching, rotation and process recovery.

Use only the owned bridge activity. Restore the original volume/rotation values.
Lock-screen checks run only when Android reports an insecure keyguard; never
change credentials or programmatically dismiss a secure keyguard.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import time

spec = importlib.util.spec_from_file_location('global_shade', Path(__file__).with_name('run-global-shade-validation.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    phone = module.Phone(args.adb, args.serial)
    result = {'result': 'fail', 'checks': {}}
    original = {}

    def volume():
        value = phone.command('shell', 'cmd', 'media_session', 'volume', '--stream', '3', '--get')
        match = re.search(r'volume is (\d+) in range \[\d+\.\.(\d+)\]', value)
        assert match, 'Media stream observation unavailable'
        return tuple(map(int, match.groups()))

    def restore_volume():
        # This ROM silently ignores AudioService writes from the shell UID.
        # Use the bench's existing ADB su authorization for exact restoration.
        phone.command('shell', "su -c 'cmd media_session volume --stream 3 --set " + str(original['volume']) + "'")
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if volume()[0] == original['volume']:
                return
            time.sleep(.1)
        raise AssertionError('Original media volume was not restored')

    try:
        phone.enabled(True)
        phone.app()
        original = {key: phone.command('shell', 'settings', 'get', 'system', key)
                    for key in ['accelerometer_rotation', 'user_rotation']}
        original['volume'], original['volume_max'] = volume()
        (args.output / 'before.json').write_text(json.dumps(original, indent=2) + '\n')
        phone.open(True)
        slider = phone.node(description='Media volume')
        left, top, right, bottom = map(int, re.findall(r'\d+', slider.get('bounds')))
        fraction = .75 if original['volume'] < original['volume_max'] / 2 else .25
        phone.command('shell', 'input', 'tap', str(round(left + (right - left) * fraction)), str((top + bottom) // 2))
        time.sleep(.5)
        observed, maximum = volume()
        assert observed != original['volume'] and maximum == original['volume_max']
        result['checks']['media_volume_applied_from_panel'] = True
        restore_volume()
        result['checks']['media_volume_restored'] = True
        # An external task switch must not leave the panel covering the new task.
        phone.command('shell', 'am', 'start', '-n', 'dev.makepad.octosense.quickstep/.GlobalShadeSettingsActivity')
        phone.wait_focus('GlobalShadeSettingsActivity')
        result['checks']['task_switch_dismisses_panel'] = True
        phone.app(); phone.open(True)
        phone.command('shell', 'input', 'keyevent', '187')
        phone.wait_focus('dev.makepad.octosense.quickstep/com.android.quickstep.RecentsActivity')
        result['checks']['recents_remains_available'] = True
        phone.command('shell', 'input', 'keyevent', '3')
        phone.wait_focus('dev.makepad.octosense/dev.makepad.octosense.MakepadApp')
        phone.app(); phone.open(True)
        phone.command('shell', 'settings', 'put', 'system', 'accelerometer_rotation', '0')
        phone.command('shell', 'settings', 'put', 'system', 'user_rotation', '1')
        phone.wait_focus('dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.BridgeSettingsActivity')
        time.sleep(.8)
        phone.command('shell', 'input', 'swipe', '2170', '1', '2170', '650', '450')
        phone.wait_focus('OctoSense Global Shade')
        phone.node(description='OctoSense Controls')
        result['checks']['rotation_dismisses_and_landscape_entry_works'] = True
        phone.command('shell', 'input', 'keyevent', '4')
        for key in ['user_rotation', 'accelerometer_rotation']:
            phone.command('shell', 'settings', 'put', 'system', key, original[key])
        time.sleep(1)
        phone.app(); phone.open(True)
        old_pid = phone.command('shell', 'pidof', 'dev.makepad.octosense.quickstep')
        assert old_pid.isdigit()
        phone.command('shell', 'am', 'force-stop', '--user', '0', 'dev.makepad.octosense.quickstep')
        phone.wait_focus('dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.BridgeSettingsActivity')
        result['checks']['process_death_removes_panel'] = True
        phone.enabled(True)
        new_pid = phone.command('shell', 'pidof', 'dev.makepad.octosense.quickstep')
        assert new_pid.isdigit() and old_pid != new_pid
        phone.app(); phone.open(True)
        result['checks']['service_restarts_and_panel_recovers'] = True
        keyguard = phone.command('shell', "dumpsys window policy | grep -E 'showing=|secure='")
        if 'secure=false' in keyguard:
            phone.command('shell', 'input', 'keyevent', '26')
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline and 'OctoSense Global Shade' in phone.focus():
                time.sleep(.2)
            assert 'OctoSense Global Shade' not in phone.focus(), 'Panel remains after screen-off transition'
            phone.command('shell', 'input', 'keyevent', '224')
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                keyguard = phone.command('shell', "dumpsys window policy | grep -E 'showing=|secure='")
                if 'showing=true' in keyguard and 'secure=false' in keyguard:
                    break
                time.sleep(.2)
            assert 'showing=true' in keyguard and 'secure=false' in keyguard, 'Expected insecure lock screen not observed'
            phone.command('shell', 'input', 'swipe', '1010', '1', '1010', '1250', '450')
            assert 'OctoSense Global Shade' not in phone.focus(), 'OctoSense panel entered over the lock screen'
            result['checks']['lock_screen_keeps_android_panel'] = True
            phone.command('shell', 'wm', 'dismiss-keyguard')
        else:
            result['lock_screen_coverage'] = 'Secure keyguard preserved; requires user-driven unlock testing'
        phone.enabled(True)
        phone.command('shell', 'input', 'keyevent', '3')
        phone.wait_focus('dev.makepad.octosense/dev.makepad.octosense.MakepadApp')
        result['result'] = 'pass'
    except Exception as failure:
        result['failure'] = str(failure) or repr(failure)
    finally:
        if original:
            try:
                restore_volume()
                for key in ['user_rotation', 'accelerometer_rotation']:
                    if original[key] == 'null':
                        phone.command('shell', 'settings', 'delete', 'system', key)
                    else:
                        phone.command('shell', 'settings', 'put', 'system', key, original[key])
                assert volume()[0] == original['volume']
                result['restored'] = True
            except Exception as failure:
                result['result'] = 'fail'; result['restore_error'] = str(failure)
        (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['result'] == 'pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
