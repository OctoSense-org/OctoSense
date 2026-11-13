#!/usr/bin/env python3
"""Check PDF search focus across appearance changes using a disposable form."""
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
    (library/'Search form.pdf').write_bytes(form_pdf())
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-search-focus')
    checks = []
    current = args.appearance

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

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
        row('Search form.pdf', 'Label')
        time.sleep(.8)
        native.click_row(row('Search form.pdf', 'Label'))
        row('1 / 2', 'Label')
        key('KeyF', cmd=1)
        native.call('t', t='needle', wait=1)
        search_value('needle')
        flip()
        native.call('t', t='more', wait=1)
        search_value('needlemore')
        native.capture('search-after-restyle')
        checks.append('Search text and caret survive an appearance change')

        key('KeyA', cmd=1)
        key('Backspace')
        native.wait(lambda: (field if (field := native.find(identifier='search', kind='TextInput'))
                             and field.get('t') != 'needlemore' else None), timeout=30)
        flip()
        native.call('t', t='fresh', wait=1)
        search_value('fresh')
        checks.append('An empty search retains focus after an appearance change')

        native.click_row(row('Fill & Sign'))
        native.click_row(row('Editable', 'Label'))
        row('before', 'TextInput')
        key('KeyA', cmd=1)
        native.call('t', t='after', wait=1)
        row('after', 'TextInput')
        key('KeyF', cmd=1)
        key('KeyA', cmd=1)
        native.call('t', t='field', wait=1)
        search_value('field')
        flip()
        native.call('t', t='more', wait=1)
        search_value('fieldmore')
        row('after', 'TextInput')
        checks.append('Search retains focus without changing an active PDF field')

        native.click_row(row('after', 'TextInput'))
        native.call('t', t='!', wait=1)
        row('after!', 'TextInput')
        key('ReturnKey')
        row('Edited', 'Label')
        checks.append('The PDF field remains editable after the search')

        key('KeyS', cmd=1)
        row('Saved', 'Label')
        key('KeyW', cmd=1)
        row('Recent', 'Label')
        time.sleep(.8)
        native.click_row(row('Search form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Fill & Sign'))
        native.click_row(row('Editable', 'Label'))
        row('after!', 'TextInput')
        checks.append('The field edit survives save and reopen')

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
