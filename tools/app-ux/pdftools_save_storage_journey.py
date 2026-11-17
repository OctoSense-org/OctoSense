#!/usr/bin/env python3
"""Check PDF save recovery using a disposable, temporarily read-only library."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import time

from capture import AppNative, ROOT
from pdftools_readonly_journey import form_pdf


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    parser.add_argument('--narrow', action='store_true', help='also use a 700-point window')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    library = state/'apps/os.pdftools/accounts/device/library'
    library.mkdir(parents=True)
    document = library/'Reference form.pdf'
    document.write_bytes(form_pdf())
    original = hashlib.sha256(document.read_bytes()).hexdigest()
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-save-storage')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def resize(width):
        for _ in range(3):
            x, y, w, h = native.wait(lambda: native.find(kind='MpModuleView'), timeout=30)['r']
            if abs(w - width) < 2:
                return
            start, end = x + w - 2, x + width - 2
            native.call('m', k='move', x=start, y=y+h/2, wait=1)
            native.call('m', k='down', x=start, y=y+h/2, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=start + (end-start)*step/6, y=y+h/2, wait=1)
            native.call('m', k='up', x=end, y=y+h/2, wait=1)
            time.sleep(.6)
        native.wait(lambda: abs(native.find(kind='MpModuleView')['r'][2] - width) < 12, timeout=30)

    error = "Can't save. Check storage permissions, then try again."
    try:
        row('Reference form.pdf', 'Label')
        time.sleep(.8)
        native.click_row(row('Reference form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Fill & Sign'))
        native.click_row(row('Editable', 'Label'))
        native.click_row(row('before', 'TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t='edited', wait=1)
        row('Edited', 'Label')
        library.chmod(0o500)
        key('KeyS', cmd=1)
        row(error, 'Label')
        row('Try Again')
        assert hashlib.sha256(document.read_bytes()).hexdigest() == original
        name = row('Editable', 'Label')
        native.wait(lambda: any(r.get('ty') == 'Label' and r.get('t') == 'edited'
                                and r['r'][0] > name['r'][0] for r in native.rows()), timeout=30)
        checks.append('A refused save preserves the original file and the visible edit')

        time.sleep(9)
        row(error, 'Label')
        row('Try Again')
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row(error, 'Label')
        row('Try Again')
        if args.narrow:
            resize(700)
            row('Try Again')
        native.capture('save-refused')
        checks.append('The failure and retry survive an appearance change and remain visible')

        native.click_row(row('Try Again'))
        time.sleep(.8)
        row(error, 'Label')
        assert hashlib.sha256(document.read_bytes()).hexdigest() == original
        library.chmod(0o700)
        native.click_row(row('Try Again'))
        row('Saved', 'Label')
        native.wait(lambda: native.find(text='Try Again') is None, timeout=30)
        key('KeyW', cmd=1)
        row('Recent', 'Label')
        time.sleep(.8)
        native.click_row(row('Reference form.pdf', 'Label'))
        native.wait(lambda: any(r.get('ty') == 'Label' and r.get('t') in ('1 / 2', '2 / 2')
                                for r in native.rows()), timeout=30)
        native.call('event', data='mac-row:mac.app.mode_fill')
        native.click_row(row('Editable', 'Label'))
        row('edited', 'TextInput')
        checks.append('Repeated refusal stays actionable; retry saves the edit for reopening')

        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'narrow': args.narrow, 'checks': checks,
            'fixture': 'generated local PDF and read-only disposable library directory'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        library.chmod(0o700)
        native.close()


if __name__ == '__main__':
    main()
