#!/usr/bin/env python3
"""Check Maps' preference-write failure and retry with disposable state."""
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
    prefs = data/'prefs.json'
    original = json.dumps({'lat': 37.335, 'lon': -121.885, 'zoom': 13, 'style': 'Standard',
                           'tilted': False, 'side': True, 'units': 'mi'})
    prefs.write_text(original)
    prefs.chmod(0o444)
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-maps']
    native = None
    checks = []
    notice = "Couldn't save map settings. Try Again."

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def clean_log():
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))

    try:
        native = AppNative(args.binary.resolve(), actions, out, 'maps-prefs-storage')
        row('Standard')
        native.call('event', data='mac-row:mac.app.style_dark')
        row('Dark')
        row(notice, 'Label')
        assert prefs.read_text() == original
        checks.append('A refused write is explained and the current style remains visible')
        native.click_row(ident('storage_retry', 'Button'))
        row(notice, 'Label')
        assert prefs.read_text() == original and native.find(text='Dark')
        checks.append('A refused retry keeps the current style and original file')
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row(notice, 'Label')
        row('Dark')
        time.sleep(9)
        row(notice, 'Label')
        checks.append('The warning and chosen map style survive appearance changes and waiting')
        native.capture('preference-write-refused')
        prefs.chmod(0o644)
        native.click_row(ident('storage_retry', 'Button'))
        native.wait(lambda: json.loads(prefs.read_text()).get('style') == 'Dark' and
                    not native.find(text=notice, kind='Label'), timeout=30)
        checks.append('Retry writes the selected style and clears the warning')
        clean_log()
        native.close()
        native = AppNative(args.binary.resolve(), actions, out, 'maps-prefs-reopen')
        row('Dark')
        assert json.loads(prefs.read_text()).get('style') == 'Dark'
        checks.append('The saved map style survives reopening')
        prefs.chmod(0o444)
        native.call('event', data='mac-row:mac.app.style_light')
        row(notice, 'Label')
        prefs.chmod(0o644)
        native.call('event', data='mac-row:mac.app.style_standard')
        native.wait(lambda: json.loads(prefs.read_text()).get('style') == 'Standard' and
                    not native.find(text=notice, kind='Label'), timeout=30)
        checks.append('A later successful settings change clears the warning')
        native.capture('preference-after-recovery')
        clean_log()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'checks': checks,
            'fixture': 'disposable read-only Maps preferences file'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
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
