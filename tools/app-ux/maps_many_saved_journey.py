#!/usr/bin/env python3
"""Check Maps with 100 disposable saved places."""
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
    data = state/'apps/os.maps/accounts/device'
    data.mkdir(parents=True)
    saved = data/'saved.json'
    saved.write_text(json.dumps([{'name': 'Synthetic Place %03d' % i, 'cat': 'Cafe',
        'label': 'Sample City', 'lat': 37.335+i*.0001, 'lon': -121.885} for i in range(100)]))
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-maps']
    native = None
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def clean_log():
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))

    try:
        native = AppNative(args.binary.resolve(), actions, out, 'maps-many-saved')
        row('Synthetic Place 000', 'Label')
        assert not native.find(text="Saved Places couldn't be loaded. Try Again before saving.", kind='Label')
        checks.append('The saved list loads and shows its first place')
        native.click_row(ident('search', 'TextInput'))
        for code in ('Escape', 'ArrowDown', 'ReturnKey'):
            native.call('k', k='press', c=code, wait=1)
        row('Directions')
        assert ident('pname', 'Label').get('t') == 'Synthetic Place 000'
        checks.append('Keyboard navigation opens the first place')
        native.call('event', data='mac-row:mac.app.close_place')
        native.wait(lambda: not native.find(text='Directions'), timeout=30)
        area = ident('results')['r']
        for _ in range(14):
            if native.find(text='Synthetic Place 099', kind='Label'):
                break
            native.call('m', k='scroll', x=area[0]+area[2]/2, y=area[1]+area[3]/2, dy=600, wait=1)
            time.sleep(.1)
        last = row('Synthetic Place 099', 'Label')
        checks.append('Scrolling reaches the last place')
        native.capture('last-place-in-list')
        x, y, width, height = last['r']
        for action in ('move', 'down', 'up'):
            params = {'k': action, 'x': x+width/2, 'y': y+height/2, 'wait': 1}
            if action != 'move':
                params['b'] = 1
            native.call('m', **params)
        row('Remove from Saved', 'PopupMenuItem')
        checks.append('The saved row offers its correct context menu')
        native.call('k', k='press', c='Escape', wait=1)
        native.click_row(row('Synthetic Place 099', 'Label'))
        row('Directions')
        row('Saved')
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row('Synthetic Place 099', 'Label')
        native.click_row(row('Saved'))
        row('Save')
        remaining = json.loads(saved.read_text())
        assert len(remaining) == 99 and all(p['name'] != 'Synthetic Place 099' for p in remaining)
        checks.append('After an appearance change, removing one place preserves the other 99')
        native.click_row(row('Save'))
        row('Saved')
        restored = json.loads(saved.read_text())
        assert len(restored) == 100 and restored[-1]['name'] == 'Synthetic Place 099'
        checks.append('Saving the place again restores the full list')
        native.capture('after-save')
        clean_log()
        native.close()
        native = AppNative(args.binary.resolve(), actions, out, 'maps-many-saved-reopen')
        row('Synthetic Place 000', 'Label')
        assert len(json.loads(saved.read_text())) == 100
        checks.append('The 100 saved places remain after reopening')
        clean_log()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'checks': checks,
            'fixture': '100 disposable synthetic saved places'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        if native:
            native.capture('failed-state')
        raise
    finally:
        if native:
            native.close()


if __name__ == '__main__':
    main()
