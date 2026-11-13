#!/usr/bin/env python3
"""Check recovery from unreadable Maps preferences in disposable state."""
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
    parser.add_argument('--edits', action='store_true', help='Change settings before restoring access')
    parser.add_argument('--corrupt', action='store_true', help='Start with a truncated file and repair it before retry')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    data = state/'apps/os.maps/accounts/device'
    data.mkdir(parents=True)
    prefs = data/'prefs.json'
    original = {'lat': 40.75, 'lon': -73.99, 'zoom': 11, 'style': 'Dark',
                'tilted': True, 'side': False, 'units': 'km'}
    original_text = json.dumps(original)
    initial_text = '{"style": "Dark",' if args.corrupt else original_text
    prefs.write_text(initial_text)
    if not args.corrupt:
        prefs.chmod(0o000)
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-maps']
    native = None
    checks = []
    notice = "Map settings couldn't be loaded. Try Again."

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def clean_log():
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))

    try:
        native = AppNative(args.binary.resolve(), actions, out, 'maps-prefs-read')
        row(notice, 'Label')
        native.click_row(ident('storage_retry', 'Button'))
        row(notice, 'Label')
        checks.append('An unavailable settings file is explained and a refused retry keeps the warning')
        if args.edits:
            native.call('event', data='mac-row:mac.app.style_light')
            row('Light')
            native.call('event', data='mac-row:mac.app.sidebar')
            native.wait(lambda: not native.find(identifier='search', kind='TextInput'), timeout=30)
            native.call('event', data='mac-row:mac.app.sidebar')
            ident('search', 'TextInput')
            native.call('event', data='mac-row:mac.app.units_km')
            def scale_uses(*suffixes):
                label = native.find(identifier='map_scale_words', kind='Label')
                return label and label.get('t', '').endswith(suffixes)
            native.wait(lambda: scale_uses(' m', ' km'), timeout=30)
            target = 'macos' if args.appearance == 'dark' else 'macos-dark'
            start = len(native.log_path.read_text())
            native.call('event', data='mac-row:mac.app.units_mi')
            native.wait(lambda: scale_uses(' ft', ' mi'), timeout=30)
            native.call('event', data='mac-row:mac.appearance.flip')
            def restyled():
                log = native.log_path.read_text()[start:]
                at = log.find('desktop style ' + target + ' applied to ')
                return at >= 0 and '[SPLASH] eval:' in log[at:]
            native.wait(restyled, timeout=30)
            time.sleep(1)  # Let the new run attempt the still-unavailable read.
            row(notice, 'Label')
            row('Light')
            checks.append('New choices survive an appearance change while the old file is unreadable')
        prefs.chmod(0o644)
        assert prefs.read_text() == initial_text
        checks.append('The unavailable file is not replaced before recovery')
        if args.corrupt:
            prefs.write_text(original_text)
        native.click_row(ident('storage_retry', 'Button'))
        native.wait(lambda: not native.find(text=notice, kind='Label'), timeout=30)
        expected = 'Light' if args.edits else 'Dark'
        row(expected)
        if args.edits:
            native.wait(lambda: json.loads(prefs.read_text()).get('style') == 'Light', timeout=30)
            saved = json.loads(prefs.read_text())
            assert (saved.get('units') == 'mi' and saved.get('side') is True and saved.get('tilted') is True and
                    abs(saved.get('lat', 0) - 40.75) < .001 and abs(saved.get('lon', 0) + 73.99) < .001 and
                    abs(saved.get('zoom', 0) - 11) < .001)
            checks.append('Retry merges changed choices with untouched preferences')
        else:
            assert prefs.read_text() == original_text
            checks.append('Retry loads untouched settings without replacing the file')
        native.capture('preferences-recovered')
        clean_log()
        native.close()
        native = AppNative(args.binary.resolve(), actions, out, 'maps-prefs-read-reopen')
        row(expected)
        saved = json.loads(prefs.read_text())
        assert saved.get('tilted') is True and saved.get('units') == ('mi' if args.edits else 'km')
        assert saved.get('side') is args.edits
        checks.append('The recovered preferences survive reopening')
        clean_log()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'edits': args.edits,
            'checks': checks, 'fixture': 'disposable truncated' if args.corrupt else 'disposable unreadable',
            'corrupt': args.corrupt}, indent=2) + '\n')
        print('PASS:', args.appearance, 'edited' if args.edits else 'untouched',
              'truncated' if args.corrupt else 'unreadable', len(checks), 'checks')
    except Exception:
        if native:
            native.capture('failed-state')
        raise
    finally:
        prefs.chmod(0o644)
        if native:
            native.close()


if __name__ == '__main__':
    main()
