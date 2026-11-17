#!/usr/bin/env python3
"""Exercise Mac Glance grouping and keyboard targeting with synthetic notices."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import time

from capture import AppNative, ROOT


def wait_for(predicate, description, timeout=30):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = predicate()
        if value:
            return value
        time.sleep(.15)
    raise AssertionError('timed out waiting for ' + description)


def text(app, identifier):
    return next((row.get('t', '') or '' for row in app.call('snap')['s']
                 if row.get('i') == identifier), '')


def log(app):
    return app.log_path.read_text(errors='replace')


def keys(app):
    match = re.match(r'\d+ notification\(s\): ([^;]*)', text(app, 'shell_glance'))
    return match.group(1).split(', ') if match and match.group(1) else []


def press(app, code, **modifiers):
    app.call('k', k='press', c=code, wait=1, **modifiers)
    time.sleep(.5)


def click(app, x, y):
    app.call('m', k='move', x=x, y=y, wait=1)
    app.call('m', k='down', x=x, y=y, wait=1)
    app.call('m', k='up', x=x, y=y, wait=1)
    time.sleep(.6)


def fixture(binary, out, mode, name, single=False):
    state = out / 'states' / name
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                      OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
                      MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
                      OCTOSENSE_MAIL_VAULT='file', OCTOSENSE_GLANCE_DEMO='0',
                      MAKEPAD_APP_CONFIG=json.dumps({'mail_demo': True}),
                      MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'style:macos-dark' if mode == 'dark' else 'style:macos'
    letters = 'AB' if single else 'ABCDEFGH'
    actions = [style, 'window-size:700x560']
    actions += [f'glance-notice:{"os.calendar" if single or index >= 4 else "os.maps"}|Sample {letter}|'
                f'A fictional notification {letter}. This local fixture has enough text for scrolling.'
                for index, letter in enumerate(letters)]
    arguments = [part for action in actions for part in ('--test-action', action)]
    app = AppNative(binary, arguments, out / 'logs', name)
    try:
        wait_for(lambda: text(app, 'desktop_shelf'), 'the desktop')
        wait_for(lambda: sum('wm: test notice from ' in line and 'Ok("published")' in line
                             for line in log(app).splitlines()) == len(letters),
                 'all synthetic notices', 60)
        app.evidence = out / 'captures'
        return app
    except BaseException:
        app.close()
        raise


def pointer(binary, out, mode):
    app = fixture(binary, out, mode, 'pointer')
    try:
        press(app, 'F9')
        wait_for(lambda: 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'Calendar stack')
        assert 'os.maps=4:collapsed' in text(app, 'shell_glance')
        app.capture('pointer-collapsed')
        click(app, 520, 120)
        wait_for(lambda: 'os.calendar=4:expanded' in text(app, 'shell_glance'), 'expanded stack')
        app.capture('pointer-expanded')
        click(app, 575, 87)
        wait_for(lambda: 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'collapsed stack')
        press(app, 'ReturnKey')
        wait_for(lambda: 'os.calendar=4:expanded' in text(app, 'shell_glance'), 'keyboard expansion')
        click(app, 660, 87)
        wait_for(lambda: 'os.calendar' not in text(app, 'shell_glance'), 'Calendar group cleared')
        assert 'os.maps=4:collapsed' in text(app, 'shell_glance'), 'another publisher was cleared'
        press(app, 'KeyZ', cmd=1)
        wait_for(lambda: 'os.calendar=4:expanded' in text(app, 'shell_glance'), 'group Undo')
        app.capture('pointer-undo')
        app.check_logs()
    finally:
        app.close()


def keyboard(binary, out, mode):
    app = fixture(binary, out, mode, 'keyboard')
    try:
        press(app, 'F9')
        wait_for(lambda: 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'Calendar stack')
        press(app, 'Delete')
        assert 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'quiet focus deleted a stack'
        press(app, 'ArrowDown')
        press(app, 'Delete')
        wait_for(lambda: 'os.calendar' not in text(app, 'shell_glance'), 'keyboard group clear')
        assert 'os.maps=4:collapsed' in text(app, 'shell_glance'), 'another publisher was cleared'
        press(app, 'KeyZ', cmd=1)
        wait_for(lambda: 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'group Undo')
        press(app, 'ReturnKey')
        wait_for(lambda: 'os.calendar=4:expanded' in text(app, 'shell_glance'), 'keyboard expansion')
        first = keys(app)[0]
        press(app, 'ArrowDown')
        before = len(log(app))
        press(app, 'Delete')
        wait_for(lambda: 'os.calendar=3:expanded' in text(app, 'shell_glance'), 'one item cleared')
        assert f'wm: glance card {first} dismissed (Delete)' in log(app)[before:]
        press(app, 'KeyZ', cmd=1)
        wait_for(lambda: 'os.calendar=4:expanded' in text(app, 'shell_glance'), 'item Undo')
        press(app, 'ArrowLeft')
        wait_for(lambda: 'os.calendar=4:collapsed' in text(app, 'shell_glance'), 'keyboard collapse')
        app.call('event', data='glance-notice:os.calendar|Fresh I|A synthetic new notification.', wait=1)
        wait_for(lambda: 'os.calendar=5:collapsed' in text(app, 'shell_glance'), 'new grouped notice')
        assert keys(app)[0] == 'os.calendar/test-FreshI', keys(app)
        app.check_logs()
    finally:
        app.close()


def singleton(binary, out, mode):
    app = fixture(binary, out, mode, 'singleton', single=True)
    try:
        press(app, 'F9')
        wait_for(lambda: 'os.calendar=2:collapsed' in text(app, 'shell_glance'), 'two-card stack')
        press(app, 'ReturnKey')
        wait_for(lambda: 'os.calendar=2:expanded' in text(app, 'shell_glance'), 'expanded two-card stack')
        shown = keys(app)
        assert len(shown) == 2, shown
        first, remaining = shown
        press(app, 'ArrowDown')
        before = len(log(app))
        press(app, 'Delete')
        wait_for(lambda: keys(app) == [remaining], 'the remaining single card')
        assert f'wm: glance card {first} dismissed (Delete)' in log(app)[before:]
        assert 'groups:' not in text(app, 'shell_glance'), text(app, 'shell_glance')
        press(app, 'KeyZ', cmd=1)
        wait_for(lambda: 'os.calendar=2:expanded' in text(app, 'shell_glance'), 'two-card stack restored')
        press(app, 'ArrowDown')
        before = len(log(app))
        press(app, 'Delete')
        wait_for(lambda: keys(app) == [remaining], 'the single card after a second dismissal')
        assert f'wm: glance card {first} dismissed (Delete)' in log(app)[before:]
        before = len(log(app))
        press(app, 'ReturnKey')
        wait_for(lambda: f'wm: glance card {remaining} opens its app (Return)' in log(app)[before:],
                 'the remaining card to open')
        app.check_logs()
    finally:
        app.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / 'logs').mkdir()
    (out / 'captures').mkdir()
    binary = args.binary.resolve()
    results = {}
    for name, check in [('pointer', pointer), ('keyboard', keyboard), ('singleton', singleton)]:
        try:
            check(binary, out, args.appearance)
            results[name] = {'passed': True}
            print('PASS', name, flush=True)
        except Exception as error:
            results[name] = {'passed': False, 'error': str(error)}
            print('FAIL', name, str(error), flush=True)
    digest = hashlib.sha256()
    with binary.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    (out / 'receipt.json').write_text(json.dumps({
        'appearance': args.appearance, 'launcher_sha256': digest.hexdigest(),
        'fixture': 'synthetic notices and disposable profiles', 'results': results,
    }, indent=2) + '\n')
    if not all(result['passed'] for result in results.values()):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
