#!/usr/bin/env python3
"""Check recovery from an unreadable disposable Maps place list."""
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
    parser.add_argument('--list', choices=['saved', 'recents'], default='saved')
    parser.add_argument('--narrow', action='store_true', help='Also check the notice at the 480-point window minimum')
    parser.add_argument('--corrupt', action='store_true', help='Start with a truncated list and repair it before retry')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    data = state/'apps/os.maps/accounts/device'
    data.mkdir(parents=True)
    saved, recent = data/'saved.json', data/'recents.json'
    saved_text = json.dumps([{'id': '', 'name': 'Saved Original', 'cat': 'Cafe',
                              'label': 'Sample City', 'lat': 37.3321, 'lon': -121.8886}])
    recent_text = json.dumps([{'id': '', 'name': 'Recent Other', 'cat': 'Cafe',
                               'label': 'Sample City', 'lat': 37.3318, 'lon': -121.9286}])
    saved.write_text(saved_text)
    recent.write_text(recent_text)
    refused = saved if args.list == 'saved' else recent
    expected = saved_text if args.list == 'saved' else recent_text
    initial = '[{"name":"Incomplete",' if args.corrupt else expected
    if args.corrupt:
        refused.write_text(initial)
    else:
        refused.chmod(0o000)
    notice = ("Saved Places couldn't be loaded. Try Again before saving." if args.list == 'saved' else
              "Recents couldn't be loaded. Try Again before updating them.")
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = None
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

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

    def other_appearance():
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        time.sleep(.5)

    try:
        native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
            '--test-action', 'launch-maps'], out, 'maps-read-storage')
        row(notice, 'Label')
        other = 'Recent Other' if args.list == 'saved' else 'Saved Original'
        row(other, 'Label')
        checks.append('The read failure is explained while the other list remains available')
        if args.narrow:
            resize(480)
            if not native.find(identifier='search', kind='TextInput'):
                native.call('event', data='mac-row:mac.app.sidebar')
            ident('search', 'TextInput')
            note = ident('storage_note')['r']
            text = ident('storage_text', 'Label')['r']
            button = ident('storage_retry', 'Button')['r']
            assert text[2] >= 110 and text[0] >= note[0] and text[0]+text[2] <= note[0]+note[2]+1
            assert button[0] >= note[0] and button[0]+button[2] <= note[0]+note[2]+1
            checks.append('The warning and retry remain readable at minimum width')
        native.capture('read-refused')
        native.click_row(ident('storage_retry', 'Button'))
        row(notice, 'Label')
        checks.append('A refused retry keeps the warning')
        native.click_row(row(other, 'Label'))
        row('Directions')
        row(notice, 'Label')
        checks.append('A place from the readable list still opens')
        other_appearance()
        row(notice, 'Label')
        checks.append('The read warning survives an appearance change')
        refused.chmod(0o644)
        if args.list == 'saved':
            native.click_row(row('Save'))
            time.sleep(.4)
            assert saved.read_text() == initial and native.find(text='Save') and native.find(text=notice, kind='Label')
            checks.append('Saving before reload leaves the original saved file intact')
        else:
            assert recent.read_text() == initial
            checks.append('Opening a place does not replace unread recents')
        if args.corrupt:
            refused.write_text(expected)
        native.click_row(ident('storage_retry', 'Button'))
        native.wait(lambda: not native.find(text=notice, kind='Label'), timeout=30)
        assert saved.read_text() == saved_text and (args.list == 'saved' or recent.read_text() == recent_text)
        checks.append('Retry reloads the original list after access returns')
        if args.list == 'saved':
            native.click_row(row('Save'))
            row('Saved')
            assert [p['name'] for p in json.loads(saved.read_text())] == ['Saved Original', 'Recent Other']
            checks.append('Saving after reload keeps both places')
        else:
            native.call('event', data='mac-row:mac.app.close_place')
            native.wait(lambda: not native.find(text='Directions'), timeout=30)
            native.click_row(row('Recent Other', 'Label'))
            row('Directions')
            assert [p['name'] for p in json.loads(recent.read_text())] == ['Recent Other']
            checks.append('Recent places update after reload')
        native.capture('after-recovery')
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'list': args.list,
            'checks': checks, 'fixture': 'disposable truncated file' if args.corrupt else 'disposable unreadable file',
            'corrupt': args.corrupt}, indent=2) + '\n')
        print('PASS:', args.appearance, args.list, len(checks), 'checks')
    except Exception:
        if native:
            native.capture('failed-state')
        raise
    finally:
        refused.chmod(0o644)
        if native:
            native.close()


if __name__ == '__main__':
    main()
