#!/usr/bin/env python3
"""Check which notification receives Return and Delete after reopening Glance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import time

from capture import AppNative, ROOT


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()


def wait_for(predicate, description, timeout=30):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = predicate()
        if value:
            return value
        time.sleep(.15)
    raise AssertionError('timed out waiting for ' + description)


def widget_text(app, identifier):
    return next((row.get('t', '') or '' for row in app.call('snap')['s']
                 if row.get('i') == identifier), '')


def log(app):
    return app.log_path.read_text(errors='replace')


def visible_keys(app):
    match = re.match(r'\d+ notification\(s\): ([^;]*)', widget_text(app, 'shell_glance'))
    return match.group(1).split(', ') if match else []


def scroll_position(app):
    matches = re.findall(r'glance panel: list at (\d+) of (\d+)', log(app))
    return tuple(map(int, matches[-1])) if matches else (0, 0)


def press(app, code, **modifiers):
    app.call('k', k='press', c=code, wait=1, **modifiers)
    time.sleep(.4)


def scenario(binary, out, mode, scroll, operation):
    name = scroll + '-' + operation
    state = out / 'states' / name
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                      OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
                      MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
                      OCTOSENSE_MAIL_VAULT='file', OCTOSENSE_GLANCE_DEMO='0',
                      MAKEPAD_APP_CONFIG=json.dumps({'mail_demo': True}),
                      MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'style:macos-dark' if mode == 'dark' else 'style:macos'
    actions = [style, 'window-size:700x560']
    actions += [f'glance-notice:{"os.maps" if index < 4 else "os.calendar"}|Sample {letter}|'
                f'A fictional notification {letter}. This longer local fixture provides enough text '
                'to exercise scrolling through several notification cards.'
                for index, letter in enumerate('ABCDEFGH')]
    arguments = [part for action in actions for part in ('--test-action', action)]
    app = AppNative(binary, arguments, out / 'logs', name)
    try:
        wait_for(lambda: widget_text(app, 'desktop_shelf'), 'the desktop')
        wait_for(lambda: sum('wm: test notice from ' in line and 'Ok("published")' in line
                             for line in log(app).splitlines()) == 8,
                 'eight synthetic notices', 60)
        press(app, 'F9')
        first = wait_for(lambda: visible_keys(app), 'top notifications')[0]
        owner = first.split('/', 1)[0]
        def group_state(state):
            return re.search(re.escape(owner) + r'=\d+:' + state, widget_text(app, 'shell_glance'))
        grouped = bool(group_state('collapsed'))
        if grouped:
            press(app, 'ReturnKey')
            wait_for(lambda: group_state('expanded'), 'top group to expand')
            press(app, 'ArrowDown')
        wait_for(lambda: scroll_position(app)[1] > 0, 'a scrollable list')
        time.sleep(.8)
        app.evidence = out / 'captures'
        app.capture(name + '-top-before')
        if scroll == 'wheel':
            app.call('m', k='scroll', x=520, y=320, dy=4000, wait=1)
            time.sleep(.8)
        else:
            for _ in range(9):
                press(app, 'ArrowDown')
        wait_for(lambda: scroll_position(app)[0] > 0, 'the list to scroll')
        bottom = visible_keys(app)
        assert bottom and bottom[0] != first, f'list did not scroll past {first}: {bottom}'
        app.capture(name + '-scrolled')
        press(app, 'Escape')
        wait_for(lambda: widget_text(app, 'shell_glance') == '', 'the panel to close')
        press(app, 'F9')
        reopened = wait_for(lambda: visible_keys(app), 'reopened notifications')
        assert reopened[0] == first, f'the reopened list starts at {reopened[0]}, expected {first}'
        app.capture(name + '-reopened')
        if grouped:
            wait_for(lambda: group_state('collapsed'), 'the reopened group')
            press(app, 'ReturnKey')
            wait_for(lambda: group_state('expanded'), 'the reopened group to expand')
            press(app, 'ArrowDown')
        if operation == 'return':
            before = len(log(app))
            press(app, 'ReturnKey')
            pattern = r'wm: glance card (\S+) opens its app \(Return\)'
        else:
            if not grouped:
                press(app, 'ArrowDown')
            before = len(log(app))
            press(app, 'Delete')
            pattern = r'wm: glance card (\S+) dismissed \(Delete\)'
        def selected_card():
            match = re.search(pattern, log(app)[before:])
            return match.group(1) if match else None
        selected = wait_for(selected_card, operation + ' to act on a notification', 20)
        assert selected == first, f'{operation} acted on {selected}, but the visible top card was {first}'
        if operation == 'delete':
            assert first not in visible_keys(app), 'the card is still shown after Delete'
            press(app, 'KeyZ', cmd=1)
            wait_for(lambda: first in visible_keys(app), 'Undo to restore the card')
            app.capture(name + '-undo')
        app.check_logs()
        return {'passed': True, 'top': first, 'scrolled_first': bottom[0], 'selected': selected}
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
    for scroll in ('wheel', 'keyboard'):
        for operation in ('return', 'delete'):
            name = scroll + '-' + operation
            try:
                results[name] = scenario(binary, out, args.appearance, scroll, operation)
                print('PASS', name, flush=True)
            except Exception as error:
                results[name] = {'passed': False, 'error': str(error)}
                print('FAIL', name, str(error), flush=True)
    (out / 'receipt.json').write_text(json.dumps({
        'appearance': args.appearance, 'launcher_sha256': digest(binary),
        'fixture': 'synthetic notices and disposable profiles', 'results': results,
    }, indent=2) + '\n')
    if not all(result['passed'] for result in results.values()):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
