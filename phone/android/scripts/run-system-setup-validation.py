#!/usr/bin/env python3
"""Exercise rendered Home settings links; requires validation Home and a revoked listener.
Does not grant permissions, change network settings, or capture another app's screen.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
from notification_flow_driver import NotificationFlowDriver


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    adb = [args.adb, '-s', args.serial]
    home = 'dev.makepad.octosense/.MakepadApp'
    report = {'result': 'fail', 'routes': {}}

    def command(*parts):
        deadline = time.monotonic() + 25
        while True:
            try:
                return subprocess.check_output(adb + list(parts), text=True, stderr=subprocess.STDOUT, timeout=30).strip()
            except subprocess.CalledProcessError as error:
                message = error.output.lower()
                read_or_cleanup = parts[0] == 'forward' or parts[:2] in [('shell', 'settings'), ('shell', 'am')] or (parts[0] == 'shell' and parts[1].startswith('dumpsys'))
                disconnected = 'no devices' in message or any(word in message for word in ['device offline', 'device not found', 'device \'cfb7c9e3\' not found'])
                if not read_or_cleanup or not disconnected or time.monotonic() >= deadline:
                    raise
                report['adb_reconnect_retries'] = report.get('adb_reconnect_retries', 0) + 1
                time.sleep(.5)

    def top():
        dump = command('shell', "dumpsys activity activities | grep -E 'topResumedActivity|mResumedActivity' || true")
        match = re.search(r'(?:topResumedActivity|mResumedActivity)[:=]\s*ActivityRecord\{[^\n]* u0 (\S+)', dump)
        if not match:
            return ''
        return match.group(1)

    def wait_top(expected):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            value = top()
            if value.startswith(expected):
                return value
            time.sleep(.2)
        raise AssertionError('Unexpected resumed activity: ' + value)

    def listener():
        return command('shell', 'settings', 'get', 'secure', 'enabled_notification_listeners')

    assert 'dev.makepad.octosense.bridge/' not in listener(), 'Use fixture-only notification validation when access is enabled'
    report['listeners_before'] = listener()
    command('shell', 'am', 'force-stop', 'dev.makepad.octosense')
    driver = NotificationFlowDriver(command, args.output, report, 0, True)
    try:
        command('shell', 'am', 'start', '-n', home, '--ez', '--remote', 'true')
        wait_top('dev.makepad.octosense/')
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            remotes = [entry for entry in driver.remotes() if entry not in driver.previous]
            if remotes:
                break
            time.sleep(.2)
        assert remotes, 'Validation remote missing'
        pid, port, token = remotes[-1]
        driver.device_port = port
        report['owned_home_pid'] = int(pid)
        driver.forward = command('forward', 'tcp:0', 'tcp:' + port)
        driver.base = 'http://127.0.0.1:' + driver.forward + '/' + token
        driver.state('home-window-ready', lambda value: value['width'] > 0)
        value = driver.renderer('home-connected', lambda r: r['connected'] and r['controls_bounds'] is not None)
        driver.tap(driver.point(value, driver.launcher(value)['notification_renderer']['controls_bounds']))

        def controls():
            return driver.renderer('controls', lambda r: r['shade_open'] > .99 and r['setup_bounds'] is not None)

        def return_home():
            command('shell', 'am', 'start', '-n', home)
            wait_top('dev.makepad.octosense/')
            driver.state('home-window-returned', lambda value: value['width'] > 0)
            controls()

        value = controls()
        summary = driver.launcher(value)['notification_renderer']['network_summary']
        assert summary and 'Connecting' not in summary
        report['network_summary'] = summary
        expected = {
            'bluetooth': 'ConnectedDeviceDashboardActivity', 'hotspot': 'TetherSettingsActivity',
            'vpn': 'VpnSettingsActivity', 'battery': 'BatterySaverSettingsActivity',
            'display': 'DisplaySettingsActivity', 'sound': 'SoundSettingsActivity',
            'accessibility': 'AccessibilitySettingsActivity',
        }
        for destination in ['internet', 'bluetooth', 'hotspot', 'vpn', 'battery', 'display', 'sound', 'accessibility']:
            value = controls()
            route = next(item for item in driver.launcher(value)['notification_renderer']['settings'] if item['destination'] == destination)
            assert route['bounds'] is not None, 'Setting is not visible: ' + destination
            driver.tap(driver.point(value, route['bounds']))
            if destination == 'internet':
                # On this ROM SettingsPanelActivity delegates to SystemUI's Internet dialog,
                # then finishes. Home remains the resumed activity beneath the panel.
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    focus = command('shell', "dumpsys window | grep mCurrentFocus")
                    if 'SystemUIDialog' in focus or 'com.android.settings/' in focus:
                        break
                    time.sleep(.2)
                else:
                    raise AssertionError('Internet panel did not open')
                report['routes'][destination] = focus
                command('shell', 'input', 'keyevent', '4')
            else:
                report['routes'][destination] = wait_top('com.android.settings/.Settings$' + expected[destination])
            print('Verified rendered setting: ' + destination, flush=True)
            return_home()
        # These tiles must open usable public settings without root.
        for toggle, activity in [('Wifi','WifiSettingsActivity'), ('Bluetooth','ConnectedDeviceDashboardActivity'), ('DoNotDisturb','ModesSettingsActivity')]:
            value = controls()
            tile = next(item for item in driver.launcher(value)['notification_renderer']['toggles'] if item['name'] == toggle)
            driver.tap(driver.point(value, tile['bounds']))
            report['routes']['tile_' + toggle] = wait_top('com.android.settings/.Settings$' + activity)
            return_home()
        value = controls()
        driver.tap(driver.point(value, driver.launcher(value)['notification_renderer']['setup_bounds']))
        report['routes']['setup'] = wait_top('dev.makepad.octosense.bridge/')
        return_home()
        # Switch shade pages with a real app-window swipe, then tap the access card.
        value = controls()
        import urllib.parse
        width, height = value['width'], value['height']
        driver.request('/swipe?' + urllib.parse.urlencode({'x1': width*.2, 'y1': height*.82, 'x2': width*.85, 'y2': height*.82}))
        time.sleep(.6)
        value = driver.renderer('notifications-setup', lambda r: any(item['destination'] == 'notifications' and item['bounds'] is not None and item['bounds'][0] >= 0 and item['bounds'][0] + item['bounds'][2] <= r['logical_width'] for item in r['settings']))
        route = next(item for item in driver.launcher(value)['notification_renderer']['settings'] if item['destination'] == 'notifications')
        driver.tap(driver.point(value, route['bounds']))
        report['routes']['notifications'] = wait_top('com.android.settings/.Settings$NotificationAccessDetailsActivity')
        command('shell', 'am', 'start', '-n', home)
        wait_top('dev.makepad.octosense/')
        report['listeners_after'] = listener()
        assert report['listeners_before'] == report['listeners_after'], 'Opening setup changed listener grants'
        report['result'] = 'pass'
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        if driver.forward and 'tcp:' + driver.forward in command('forward', '--list'):
            command('forward', '--remove', 'tcp:' + driver.forward)
        # Normal APK installation is handled by the caller after validation.
        if 'owned_home_pid' in report:
            command('shell', 'am', 'force-stop', 'dev.makepad.octosense')
            report['owned_process_stopped'] = True
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
