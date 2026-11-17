#!/usr/bin/env python3
"""Check a PDF paragraph draft across tabs, appearance and save/reopen."""
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
    for name in ('Original form.pdf', 'Other form.pdf'):
        (library/name).write_bytes(form_pdf())
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-edit')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def page():
        window = native.find(kind='MpModuleView')
        if not window:
            return None
        x, y, width, height = window['r']
        pages = [r for r in native.rows() if r.get('ty') == 'Image'
                 and r['r'][2] < width - 1 and x - 1 <= r['r'][0] <= x + width
                 and y - 1 <= r['r'][1] <= y + height]
        return min(pages, key=lambda r: r['r'][1]) if pages else None

    def select_paragraph():
        x, y, width, height = native.wait(page, timeout=60)['r']
        native.click_row({'r': [x+width*35/300, y+height*26/300, 1, 1]})

    def home(tab):
        x, _, _, _ = native.wait(lambda: native.find(kind='MpModuleView'), timeout=30)['r']
        _, y, _, height = row(tab, 'Label')['r']
        native.click_row({'r': [x+20, y+height/2, 1, 1]})
        row('Recent', 'Label')

    try:
        row('Original form.pdf', 'Label')
        time.sleep(.8)
        native.click_row(row('Original form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Edit'))
        native.call('g')
        time.sleep(.5)
        select_paragraph()
        native.click_row(row('Editable page', 'TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t='Revised page', wait=1)
        row('Revised page', 'TextInput')
        row('Editing', 'Label')
        checks.append('The paragraph draft is marked Editing')

        home('Original form.pdf')
        native.click_row(row('Other form.pdf', 'Label'))
        row('Other form', 'Label')
        native.click_row(row('Original form.pdf', 'Label'))
        row('Original form', 'Label')
        native.click_row(row('Revised page', 'TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t='Revised page!', wait=1)
        row('Revised page!', 'TextInput')
        native.capture('draft-after-tab-switch')
        checks.append('Returning from another PDF restores the editable canvas')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row('Revised page!', 'TextInput')
        time.sleep(.8)  # Let the real edit check finish before Apply.
        native.click_row(row('Apply'))
        row('Edited', 'Label')
        checks.append('The draft survives restyle and can be applied')

        key('KeyS', cmd=1)
        row('Saved', 'Label')
        key('KeyW', cmd=1)
        native.wait(lambda: not native.find(text='Original form.pdf', kind='Label'), timeout=30)
        home('Other form.pdf')
        time.sleep(.5)
        native.click_row(row('Original form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Edit'))
        native.call('g')
        time.sleep(.5)
        select_paragraph()
        row('Revised page!', 'TextInput')
        checks.append('The saved paragraph reopens')

        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'two generated local PDFs'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
