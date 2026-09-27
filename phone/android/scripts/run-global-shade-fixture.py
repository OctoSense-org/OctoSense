#!/usr/bin/env python3
"""Exercise only owned fixture notification actions in the native global panel.

Temporarily installs the bridge validation APK and grants its fixture posting
permission. Restores the supplied checksum-matched normal bridge and preserves
the user's approved ongoing notification listener and write-settings access.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import time
import uuid
import re
import shlex

spec = importlib.util.spec_from_file_location('global_shade', Path(__file__).with_name('run-global-shade-validation.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--normal-apk', type=Path, required=True)
    parser.add_argument('--validation-apk', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    phone = module.Phone(args.adb, args.serial)
    package = 'dev.makepad.octosense.bridge'
    token = uuid.uuid4().hex
    title = 'OctoSense global fixture ' + token[:6]
    report = {'result': 'fail', 'checks': {}, 'fixture_title': title, 'fixture_token': token}
    installed = False
    instrument = None
    remote_log = '/data/local/tmp/octosense-shade-fixture-' + token + '.log'
    log = args.output / 'fixture.log'

    def current_hash():
        path = phone.command('shell', 'pm', 'path', package).removeprefix('package:')
        return phone.command('shell', 'sha256sum', path).split()[0]

    def fixture(operation):
        return phone.command('shell', 'am', 'broadcast', '-a', package + '.GLOBAL_SHADE_FIXTURE',
                             '-p', package, '--es', 'token', token, '--es', 'operation', operation)

    def status():
        content = phone.command('shell', 'cat', remote_log)
        log.write_text(content + '\n')
        return dict(re.findall(r'^INSTRUMENTATION_STATUS: ([^=]+)=(.*)$', content, re.M))

    def wait_finished():
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            status()
            if 'INSTRUMENTATION_CODE:' in log.read_text():
                return
            time.sleep(.2)
        raise AssertionError('Owned fixture did not finish')

    def wait_status(key, value):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            if status().get(key) == value:
                return
            time.sleep(.2)
        raise AssertionError('Fixture state not observed: ' + key)

    def card(expected=title):
        for attempt in range(7):
            # Keep layout parents: compressed accessibility dumps flatten the
            # non-clickable ongoing card into the surrounding notification list.
            tree = phone.tree(compressed=False)
            parents = {child: parent for parent in tree.iter() for child in parent}
            matches = [node for node in tree.iter('node') if node.get('text') == expected]
            if len(matches) == 1:
                return parents[matches[0]]
            phone.command('shell', 'input', 'swipe', '540', '1620', '540', '650', '350')
            time.sleep(.2)
        raise AssertionError('Owned fixture card not visible')

    try:
        report['before_sha256'] = current_hash()
        assert report['before_sha256'] == hashlib.sha256(args.normal_apk.read_bytes()).hexdigest()
        report['listeners_before'] = phone.command('shell', 'settings', 'get', 'secure', 'enabled_notification_listeners')
        assert package + '/' in report['listeners_before']
        assert 'Success' in phone.command('install', '--no-incremental', '-r', str(args.validation_apk))
        installed = True
        phone.command('shell', 'pm', 'grant', '--user', '0', package, 'android.permission.POST_NOTIFICATIONS')
        launch = 'nohup am instrument -w -e token ' + token + ' ' + package + '/' + package + '.validation.GlobalShadeFixtureInstrumentation'
        launch += ' > ' + shlex.quote(remote_log) + ' 2>&1 < /dev/null & echo $!'
        instrument = phone.command('shell', launch)
        assert instrument.isdigit()
        report['owned_fixture_shell_pid'] = instrument
        wait_status('phase', 'ready')
        phone.enabled(True)
        phone.app()
        fixture('post'); wait_status('posted', 'true'); time.sleep(.5)
        phone.open(False)
        own = card()
        phone.tap(phone.node(text='Reply', tree=own)); time.sleep(.7)
        phone.command('shell', 'input', 'text', 'stale_draft')
        fixture('update'); wait_status('phase', 'update'); time.sleep(.5)
        tree = phone.tree()
        assert not any(n.get('class') == 'android.widget.EditText' for n in tree.iter('node'))
        assert status().get('replies') == '0'
        report['checks']['updated_notification_expires_unsent_reply'] = True
        own = card()
        phone.tap(phone.node(text='Reply', tree=own)); time.sleep(.7)
        assert sum(n.get('class') == 'android.widget.EditText' for n in phone.tree().iter('node')) == 1
        phone.command('shell', 'input', 'text', 'OctoSense_global_reply')
        ime = phone.command('shell', "dumpsys input_method | grep -E 'mInputShown=|mIsInputViewShown=' || true")
        assert 'mInputShown=true' in ime or 'mIsInputViewShown=true' in ime
        report['checks']['native_reply_keyboard_visible'] = True
        phone.tap(phone.node(text='Send'))
        wait_status('replies', '1'); assert status().get('reply') == 'OctoSense_global_reply'
        report['checks']['reply_delivered_once_to_owned_receiver'] = True
        time.sleep(.5)
        ongoing = card(title + ' ongoing')
        assert not any(n.get('text') == 'Clear' for n in ongoing.iter('node')), 'Owned ongoing card exposes Clear'
        report['checks']['ongoing_has_no_clear'] = True
        # Return to the top before locating the owned dismissible card again.
        phone.command('shell', 'input', 'swipe', '540', '650', '540', '1620', '350')
        own = card(); phone.tap(phone.node(text='Clear', tree=own)); time.sleep(.6)
        fixture('state'); wait_status('posted', 'false')
        report['checks']['owned_notification_dismissed'] = True
        phone.tap(phone.node(description='Close OctoSense panel'))
        phone.wait_focus(package + '/' + package + '.BridgeSettingsActivity')
        report['checks']['close_returns_to_underlying_app'] = True
        fixture('finish'); wait_finished()
        assert 'INSTRUMENTATION_RESULT: result=pass' in log.read_text()
        report['result'] = 'pass'
    except Exception as failure:
        report['failure'] = str(failure) or repr(failure)
    finally:
        if instrument is not None and 'INSTRUMENTATION_CODE:' not in (log.read_text() if log.exists() else ''):
            try:
                fixture('finish'); wait_finished()
            except Exception as failure:
                report['fixture_cleanup_error'] = str(failure)
        if installed:
            try:
                phone.command('shell', 'pm', 'revoke', '--user', '0', package, 'android.permission.POST_NOTIFICATIONS')
                assert 'Success' in phone.command('install', '--no-incremental', '-r', str(args.normal_apk))
                report['restored_sha256'] = current_hash()
                assert report['restored_sha256'] == report['before_sha256']
                report['listeners_after'] = phone.command('shell', 'settings', 'get', 'secure', 'enabled_notification_listeners')
                assert report['listeners_after'] == report['listeners_before']
                report['write_settings'] = phone.command('shell', 'cmd', 'appops', 'get', '--user', '0', package, 'WRITE_SETTINGS')
                assert 'WRITE_SETTINGS: allow' in report['write_settings']
                assert package not in phone.command('shell', 'pm', 'list', 'instrumentation')
                phone.command('shell', 'rm', '-f', remote_log)
                report['restored'] = True
            except Exception as failure:
                report['result'] = 'fail'; report['restore_error'] = str(failure)
        (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if report['result'] == 'pass' and report.get('restored') else 1


if __name__ == '__main__':
    raise SystemExit(main())
