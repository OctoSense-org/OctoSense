#!/usr/bin/env python3
"""Check a large disposable Maps list, paging and saved-place actions."""
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
    parser.add_argument('--narrow', action='store_true', help='Also check the pager at the 480-point window minimum')
    parser.add_argument('--count', type=int, choices=[500, 1000], default=500,
                        help='Number of disposable saved places')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    data = state/'apps/os.maps/accounts/device'
    data.mkdir(parents=True)
    saved = data/'saved.json'
    original = json.dumps([{'name': 'Synthetic Place %03d' % i, 'cat': 'Cafe',
        'label': 'Sample City', 'lat': 37.335+i*.0001, 'lon': -121.885} for i in range(args.count)])
    saved.write_text(original)
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

    def resize(width):
        for _ in range(3):
            x, y, w, h = ident(None, 'MpModuleView')['r']
            if abs(w - width) < 2:
                return
            start, end = x + w - 2, x + width - 2
            native.call('m', k='move', x=start, y=y+h/2, wait=1)
            native.call('m', k='down', x=start, y=y+h/2, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=start + (end-start)*step/6, y=y+h/2, wait=1)
            native.call('m', k='up', x=end, y=y+h/2, wait=1)
            time.sleep(.6)
        assert abs(ident(None, 'MpModuleView')['r'][2] - width) < 12

    try:
        native = AppNative(args.binary.resolve(), actions, out, 'maps-saved-paging')
        row(f'1–100 of {args.count}', 'Label')
        row('Synthetic Place 000', 'Label')
        assert not row('Previous').get('enabled', True) and saved.read_text() == original
        checks.append('The first page loads without replacing the saved file')
        scripts = list((state/'apps/.system/os.maps').glob('*/main.splash'))
        assert scripts and all(p.read_bytes() == (ROOT/'apps/maps/bundle/main.splash').read_bytes() for p in scripts)
        checks.append('The packaged Maps script matches the source under test')
        if args.narrow:
            resize(480)
            if not native.find(identifier='search', kind='TextInput'):
                native.call('event', data='mac-row:mac.app.sidebar')
            ident('search', 'TextInput')
            win = ident(None, 'MpModuleView')['r']
            for control in (row('Previous'), row(f'1–100 of {args.count}', 'Label'), row('Next')):
                x, y, w, h = control['r']
                assert x >= win[0] and x+w <= win[0]+win[2]+1 and y >= win[1] and y+h <= win[1]+win[3]+1
            checks.append('The page controls fit at minimum window width')
        native.click_row(row('Next'))
        row(f'101–200 of {args.count}', 'Label')
        row('Synthetic Place 100', 'Label')
        checks.append('Next opens the second page')
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row(f'101–200 of {args.count}', 'Label')
        row('Synthetic Place 100', 'Label')
        checks.append('Appearance changes retain the current page')
        pages = args.count // 100
        for page in range(2, pages):
            native.click_row(row('Next'))
            row(f'{page * 100 + 1}–{(page + 1) * 100} of {args.count}', 'Label')
        last_first = (pages - 1) * 100
        previous_first = last_first - 100
        row(f'Synthetic Place {last_first:03d}', 'Label')
        assert not row('Next').get('enabled', True)
        checks.append('The last page is reachable and Next is disabled there')
        native.capture('last-page')
        native.click_row(ident('search', 'TextInput'))
        for code in ('Escape', 'ArrowDown', 'ReturnKey'):
            native.call('k', k='press', c=code, wait=1)
        row('Directions')
        assert ident('pname', 'Label').get('t') == f'Synthetic Place {last_first:03d}'
        checks.append('Keyboard navigation opens the current page’s first place')
        native.call('event', data='mac-row:mac.app.close_place')
        native.wait(lambda: not native.find(text='Directions'), timeout=30)
        row(f'{last_first + 1}–{args.count} of {args.count}', 'Label')
        native.click_row(row('Previous'))
        previous_name = f'Synthetic Place {previous_first:03d}'
        item = row(previous_name, 'Label')
        checks.append('Previous returns to the preceding page')
        x, y, width, height = item['r']
        for action in ('move', 'down', 'up'):
            params = {'k': action, 'x': x+width/2, 'y': y+height/2, 'wait': 1}
            if action != 'move':
                params['b'] = 1
            native.call('m', **params)
        row('Remove from Saved', 'PopupMenuItem')
        checks.append('A paged place keeps its context menu')
        native.call('k', k='press', c='Escape', wait=1)
        native.click_row(row(previous_name, 'Label'))
        row('Directions')
        native.click_row(row('Saved'))
        row('Save')
        assert len(json.loads(saved.read_text())) == args.count - 1
        native.click_row(row('Save'))
        row('Saved')
        restored = json.loads(saved.read_text())
        assert len(restored) == args.count and restored[-1]['name'] == previous_name
        checks.append('Removing and saving one place preserves the others')
        native.capture('after-save')
        clean_log()
        native.close()
        native = AppNative(args.binary.resolve(), actions, out, 'maps-saved-paging-reopen')
        row(f'1–100 of {args.count}', 'Label')
        row('Synthetic Place 000', 'Label')
        assert len(json.loads(saved.read_text())) == args.count
        checks.append('All saved places remain after reopening')
        clean_log()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'checks': checks,
            'fixture': f'{args.count} disposable synthetic saved places'}, indent=2) + '\n')
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
