#!/usr/bin/env python3
"""Check PDF field Undo, Redo and save/reopen on a disposable form."""
import argparse
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
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    library = state/'apps/os.pdftools/accounts/device/library'
    library.mkdir(parents=True)
    (library/'History form.pdf').write_bytes(form_pdf())
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-history')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def field_value(text):
        native.wait(lambda: any(r.get('ty') == 'Label' and r.get('t') == text
                                for r in native.rows()), timeout=60)

    try:
        row('History form.pdf', 'Label')
        time.sleep(.8)
        native.click_row(row('History form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Fill & Sign'))
        native.click_row(row('Editable', 'Label'))
        row('before', 'TextInput')
        key('KeyA', cmd=1)
        native.call('t', t='after history', wait=1)
        key('ReturnKey')
        field_value('after history')
        row('Edited', 'Label')
        checks.append('The field edit is applied')

        key('KeyZ', cmd=1)
        field_value('before')
        native.click_row(row('Editable', 'Label'))
        row('before', 'TextInput')
        native.capture('after-undo')
        checks.append('Undo restores the previous field value')
        key('ReturnKey')

        key('KeyZ', cmd=1, shift=1)
        field_value('after history')
        native.click_row(row('Editable', 'Label'))
        row('after history', 'TextInput')
        checks.append('Redo restores the field edit')
        key('ReturnKey')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        field_value('after history')
        checks.append('The redone value survives an appearance change')

        key('KeyS', cmd=1)
        row('Saved', 'Label')
        key('KeyW', cmd=1)
        row('Recent', 'Label')
        time.sleep(.8)
        native.click_row(row('History form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Fill & Sign'))
        native.click_row(row('Editable', 'Label'))
        row('after history', 'TextInput')
        checks.append('The redone field value survives save and reopen')

        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'generated local PDF'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
