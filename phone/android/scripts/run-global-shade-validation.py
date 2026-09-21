#!/usr/bin/env python3
"""Check the owned global panel over an owned app, without capturing screen pixels.

Requires the normal native Quickstep candidate and compatible System Bridge.
Only own UI controls, task identity and pass/fail values are retained. Android's
fallback panel is identified by its window, never its notification contents.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET


class Phone:
    def __init__(self, adb, serial, owned_focus=None):
        self.adb = [adb, '-s', serial]
        self.owned_focus = owned_focus or ['OctoSense Global Shade', 'dev.makepad.octosense.quickstep/', 'dev.makepad.octosense.bridge/']

    def command(self, *parts):
        retryable = parts[:2] in [('shell', 'pm'), ('shell', 'dumpsys'), ('shell', 'stat'),
                                  ('shell', 'uiautomator'), ('shell', 'rm'), ('shell', 'cat')]
        retryable |= parts[0] == 'shell' and parts[1].startswith('dumpsys ')
        # Package mutation commands are never replayed here.
        if parts[:2] == ('shell', 'pm'):
            retryable = parts[2] in ('path', 'list')
        deadline = time.monotonic() + 20
        while True:
            try:
                return subprocess.check_output(self.adb + list(parts), text=True,
                                               stderr=subprocess.STDOUT, timeout=30).strip()
            except subprocess.CalledProcessError as failure:
                disconnected = any(s in failure.output.lower() for s in ['not found', 'offline', 'no devices', 'closed'])
                if not retryable or not disconnected or time.monotonic() >= deadline:
                    raise
                time.sleep(.5)

    def focus(self):
        return self.command('shell', 'dumpsys window | grep mCurrentFocus')

    def wait_focus(self, expected):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            current = self.focus()
            if expected in current:
                return current
            time.sleep(.2)
        raise AssertionError('Unexpected focus: ' + current)

    def tree(self, compressed=True):
        assert any(p in self.focus() for p in self.owned_focus)
        remote = '/data/local/tmp/octosense-panel-' + uuid.uuid4().hex + '.xml'
        try:
            self.command('shell', 'uiautomator', 'dump', *(['--compressed'] if compressed else []), remote)
            size = int(self.command('shell', 'stat', '-c', '%s', remote))
            assert 0 < size < 600_000
            chunks = []
            for index in range((size + 4095) // 4096):
                deadline = time.monotonic() + 20
                while True:
                    read = subprocess.run(self.adb + ['exec-out',
                        f'dd if={remote} bs=4096 skip={index} count=1 2>/dev/null'], capture_output=True, timeout=15)
                    if read.returncode == 0 and len(read.stdout) == min(4096, size - index * 4096):
                        chunks.append(read.stdout); break
                    if time.monotonic() >= deadline:
                        raise AssertionError('Owned UI hierarchy transfer interrupted')
                    time.sleep(.5)
            data = b''.join(chunks)
            return ET.fromstring(data)
        finally:
            self.command('shell', 'rm', '-f', remote)

    def node(self, text=None, description=None, tree=None):
        tree = tree if tree is not None else self.tree()
        nodes = [n for n in tree.iter('node') if (text is None or n.get('text') == text)
                 and (description is None or n.get('content-desc') == description)]
        assert len(nodes) == 1, 'Expected one owned UI control: ' + str(text or description)
        return nodes[0]

    def tap(self, node):
        assert any(p in self.focus() for p in self.owned_focus)
        left, top, right, bottom = map(int, re.findall(r'\d+', node.get('bounds')))
        assert right > left and bottom > top
        self.command('shell', 'input', 'tap', str((left + right) // 2), str((top + bottom) // 2))

    def app(self):
        self.command('shell', 'am', 'start', '-n', 'dev.makepad.octosense.bridge/.BridgeSettingsActivity')
        self.wait_focus('dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.BridgeSettingsActivity')
        time.sleep(.8)
        return self.command('shell', "dumpsys activity activities | grep 'topResumedActivity.*dev.makepad.octosense.bridge/'")

    def open(self, controls=True):
        x = '1010' if controls else '70'
        self.command('shell', 'input', 'swipe', x, '1', x, '1250', '450')
        self.wait_focus('OctoSense Global Shade')
        time.sleep(.3)

    def wake_home(self):
        self.command('shell', 'input', 'keyevent', '224')
        keyguard = self.command('shell', "dumpsys window policy | grep -E 'showing=|secure='")
        if 'showing=true' in keyguard:
            assert 'secure=false' in keyguard, 'Unlock the phone normally before UI validation'
            self.command('shell', 'wm', 'dismiss-keyguard')
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                keyguard = self.command('shell', "dumpsys window policy | grep -E 'showing=|secure='")
                if 'showing=false' in keyguard:
                    break
                time.sleep(.2)
            assert 'showing=false' in keyguard, 'Keyguard dismissal has not completed'
        # Home input during the wake/keyguard exit animation can be discarded.
        time.sleep(.8)
        self.command('shell', 'input', 'keyevent', '3')
        self.wait_focus('dev.makepad.octosense/dev.makepad.octosense.MakepadApp')
        time.sleep(.8)

    def enabled(self, enabled):
        self.wake_home()
        self.command('shell', 'am', 'start', '-n', 'dev.makepad.octosense.quickstep/.GlobalShadeSettingsActivity')
        self.wait_focus('GlobalShadeSettingsActivity')
        toggle = self.node(text='Use OctoSense panel across apps')
        if (toggle.get('checked') == 'true') != enabled:
            self.tap(toggle)
        deadline = time.monotonic() + 20
        expected = 'Active ·' if enabled else "Android's panel is active"
        while time.monotonic() < deadline:
            if any(n.get('text', '').startswith(expected) for n in self.tree().iter('node')):
                return
            time.sleep(.3)
        raise AssertionError('Panel did not reach the requested state')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    phone = Phone(args.adb, args.serial)
    result = {'result': 'fail', 'checks': {}}
    try:
        phone.enabled(True)
        before = phone.app()
        phone.open(True)
        tree = phone.tree()
        phone.node(description='OctoSense Controls', tree=tree)
        for name in ['Media volume', 'Brightness']:
            phone.node(description=name, tree=tree)
        after = phone.command('shell', "dumpsys activity activities | grep 'topResumedActivity.*dev.makepad.octosense.bridge/'")
        assert before == after, 'Panel changed the resumed app or its task'
        result['checks']['controls_over_same_resumed_task'] = True
        phone.command('shell', 'input', 'keyevent', '4')
        phone.wait_focus('dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.BridgeSettingsActivity')
        result['checks']['back_returns_to_app'] = True
        phone.open(False)
        phone.node(description='OctoSense Notifications')
        phone.tap(phone.node(description='Close OctoSense panel'))
        phone.wait_focus('dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.BridgeSettingsActivity')
        result['checks']['notifications_and_close'] = True
        phone.enabled(False)
        phone.app()
        phone.command('shell', 'input', 'swipe', '1010', '1', '1010', '1250', '450')
        phone.wait_focus('NotificationShade')
        result['checks']['disabled_restores_android_panel'] = True
        phone.command('shell', 'input', 'keyevent', '3')
        phone.wait_focus('dev.makepad.octosense/dev.makepad.octosense.MakepadApp')
        phone.enabled(True)
        phone.app()
        phone.open(True)
        result['checks']['reenabled'] = True
        phone.tap(phone.node(description='Close OctoSense panel'))
        phone.command('shell', 'input', 'keyevent', '3')
        phone.wait_focus('dev.makepad.octosense/dev.makepad.octosense.MakepadApp')
        result['result'] = 'pass'
    except Exception as failure:
        result['failure'] = str(failure)
    finally:
        (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['result'] == 'pass' else 1


if __name__ == '__main__':
    raise SystemExit(main())
