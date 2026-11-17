#!/usr/bin/env python3
"""Check local Clock and Calendar widgets in a disposable Mac Glance panel."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import sys
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


def press(app, code):
    app.call('k', k='press', c=code, wait=1)
    time.sleep(.5)


def mouse(app, kind, x, y):
    app.call('m', k=kind, x=x, y=y, wait=1)
    time.sleep(.4)


def values(app):
    said = text(app, 'shell_glance')
    now = datetime.datetime.now()
    moments = [now + datetime.timedelta(minutes=m) for m in (-1, 0, 1)]
    assert any('Calendar ' + stamp.strftime('%A %B %-d') in said for stamp in moments), said
    assert any('Clock ' + stamp.strftime(fmt) in said for stamp in moments
               for fmt in ('%-I:%M %p', '%H:%M')), said
    return said


def scenario(binary, out, mode, kind):
    state = out / 'states' / kind
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                      OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
                      MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
                      OCTOSENSE_MAIL_VAULT='file', OCTOSENSE_GLANCE_DEMO='0',
                      MAKEPAD_APP_CONFIG=json.dumps({'mail_demo': True}),
                      MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'style:macos-dark' if mode == 'dark' else 'style:macos'
    actions = [style, 'window-size:480x480' if kind == 'grouped' else 'window-size:700x560']
    if kind == 'grouped':
        actions += [f'glance-notice:{"os.maps" if n < 4 else "os.calendar"}|Sample {letter}|'
                    f'A fictional notice {letter} provides enough text for scrolling.'
                    for n, letter in enumerate('ABCDEFGH')]
    arguments = [part for action in actions for part in ('--test-action', action)]
    app = AppNative(binary, arguments, out / 'logs', kind)
    try:
        wait_for(lambda: text(app, 'desktop_shelf'), 'desktop', 60)
        if kind == 'grouped':
            wait_for(lambda: sum('wm: test notice from ' in line and 'Ok("published")' in line
                                 for line in log(app).splitlines()) == 8, 'synthetic notices', 60)
        press(app, 'F9')
        wait_for(lambda: '; widgets: Calendar ' in text(app, 'shell_glance'), 'widget descriptions')
        time.sleep(.8)
        said = values(app)
        app.evidence = out / 'captures'
        app.capture(kind + '-open')
        if kind == 'calendar':
            assert said.startswith('No Notifications;'), said
            press(app, 'ArrowRight')
            press(app, 'ArrowLeft')
            press(app, 'Delete')
            assert text(app, 'shell_glance').startswith('No Notifications; widgets:'), 'Delete removed a widget'
            app.capture(kind + '-focused')
            press(app, 'ReturnKey')
            wait_for(lambda: 'wm: glance widget calendar opens its app (Return)' in log(app), 'Calendar activation')
            wait_for(lambda: 'wm: launched calendar ' in log(app), 'Calendar launch', 60)
        elif kind == 'horizontal':
            assert said.startswith('No Notifications;'), said
            press(app, 'ArrowRight')
            app.capture(kind + '-focused')
            press(app, 'ReturnKey')
            wait_for(lambda: 'wm: glance widget clock opens its app (Return)' in log(app), 'horizontal focus to Clock')
            wait_for(lambda: 'wm: launched clock ' in log(app), 'Clock launch', 60)
        elif kind == 'pointer':
            assert said.startswith('No Notifications;'), said
            mouse(app, 'move', 600, 180)
            mouse(app, 'down', 600, 180)
            assert 'wm: glance widget clock opens its app' not in log(app), 'activated before pointer release'
            assert text(app, 'shell_glance').startswith('No Notifications; widgets:'), 'panel closed on pointer down'
            app.capture(kind + '-pressed')
            mouse(app, 'move', 400, 400)
            mouse(app, 'up', 400, 400)
            assert 'wm: glance widget clock opens its app' not in log(app), 'dragging away activated the widget'
            assert text(app, 'shell_glance').startswith('No Notifications; widgets:'), 'dragging away closed the panel'
            mouse(app, 'move', 600, 180)
            mouse(app, 'down', 600, 180)
            mouse(app, 'up', 600, 180)
            wait_for(lambda: 'wm: glance widget clock opens its app (pointer)' in log(app), 'Clock activation')
            wait_for(lambda: 'wm: launched clock ' in log(app), 'Clock launch', 60)
        else:
            assert 'os.calendar=4:collapsed' in said and 'os.maps=4:collapsed' in said, said
            for _ in range(4):
                press(app, 'ArrowDown')
            app.capture(kind + '-widgets-focused')
            press(app, 'ReturnKey')
            wait_for(lambda: 'wm: glance widget clock opens its app (Return)' in log(app), 'Clock after groups')
            wait_for(lambda: 'wm: launched clock ' in log(app), 'Clock launch', 60)
        wait_for(lambda: text(app, 'shell_glance') == '', 'panel to close')
        app.check_logs()
        return {'passed': True, 'description': said}
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
    for kind in ('calendar', 'horizontal', 'pointer', 'grouped'):
        try:
            results[kind] = scenario(binary, out, args.appearance, kind)
            print('PASS', kind, flush=True)
        except Exception as error:
            results[kind] = {'passed': False, 'error': str(error)}
            print('FAIL', kind, str(error), flush=True)
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
