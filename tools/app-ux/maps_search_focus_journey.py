#!/usr/bin/env python3
"""Check Maps search focus across appearance changes with a disposable place."""
import argparse
import json
import os
from pathlib import Path
import time

from capture import AppNative, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    places = state/'apps/os.maps/accounts/device'
    places.mkdir(parents=True)
    (places/'saved.json').write_text(json.dumps([{'id': '', 'name': 'Fixture Cafe',
        'cat': 'Cafe', 'label': 'Sample City', 'lat': 37.3321, 'lon': -121.8886}]))
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-maps'], out, 'maps-search-focus')
    checks = []
    current = args.appearance

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def search_value(value):
        return native.wait(lambda: (field if (field := native.find(identifier='search', kind='TextInput'))
                                    and field.get('t') == value else None), timeout=30)

    def flip():
        nonlocal current
        target = 'macos' if current == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        current = 'light' if current == 'dark' else 'dark'
        time.sleep(.5)

    try:
        ident('search', 'TextInput')
        native.field('search', 'alpha')
        key('ArrowLeft')
        flip()
        native.call('t', t='!', wait=1)
        search_value('alph!a')
        checks.append('Search text and caret survive an appearance change')
        native.capture('search-after-restyle')

        native.click_row(ident('search_clear', 'Button'))
        native.call('t', t='after-clear', wait=1)
        search_value('after-clear')
        checks.append('The clear icon returns the keyboard to search')

        key('KeyA', cmd=1)
        key('Backspace')
        flip()
        native.call('t', t='empty', wait=1)
        search_value('empty')
        checks.append('An empty search retains focus after an appearance change')

        native.click_row(ident('map_keys', 'GestureView'))
        flip()
        native.call('t', t='Z', wait=1)
        time.sleep(.5)
        search_value('empty')
        checks.append('Changing appearance does not refocus search after a map click')

        native.click_row(ident('search_clear', 'Button'))
        native.click_row(row('Fixture Cafe', 'Label'))
        native.click_row(row('Directions'))
        native.click_row(row('Choose a Start…'))
        row('Back to Route')
        native.call('t', t='first', wait=1)
        search_value('first')
        native.click_row(ident('search_clear', 'Button'))
        native.call('t', t='start', wait=1)
        search_value('start')
        row('Back to Route')
        checks.append('Clearing the start search keeps its route context and focus')
        key('ArrowLeft')
        flip()
        row('Back to Route')
        native.call('t', t='!', wait=1)
        search_value('star!t')
        checks.append('Choosing a start preserves the search caret across appearance')
        native.capture('choose-start-after-restyle')

        key('Escape')
        row('Choose a Start', 'Label')
        checks.append('Escape returns from start search to directions')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'disposable saved place'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
