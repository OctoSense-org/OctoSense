#!/usr/bin/env python3
"""Check PDF notes, reply drafts and deletion on a disposable local form."""
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
    (library/'Comment form.pdf').write_bytes(form_pdf())
    (library.parent/'library.json').write_text(json.dumps({'version': 1, 'docs': {},
        'who': {'author': 'Test User', 'initials': 'TU'}}))
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-comment')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def right_click(item):
        x, y, width, height = item['r']
        for action in ('move', 'down', 'up'):
            query = {'k': action, 'x': x+width/2, 'y': y+height/2, 'wait': 1}
            if action != 'move':
                query['b'] = 1
            native.call('m', **query)

    try:
        row('Comment form.pdf', 'Label')
        time.sleep(.8)
        native.click_row(row('Comment form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Comment'))
        row('Note')
        native.call('g')  # Let the mode transition place the new toolbar.
        time.sleep(.5)
        native.click_row(row('Note'))
        row('No comments yet. Click the page where the note goes.', 'Label')

        def top_page():
            window = native.find(kind='MpModuleView')
            if not window:
                return None
            x, y, width, height = window['r']
            pages = [r for r in native.rows() if r.get('ty') == 'Image'
                     and r['r'][2] < width - 1 and x - 1 <= r['r'][0] <= x + width
                     and y - 1 <= r['r'][1] <= y + height]
            return min(pages, key=lambda r: r['r'][1]) if pages else None

        page = native.wait(top_page, timeout=60)['r']
        native.click_row({'r': [page[0]+page[2]/3, page[1]+page[3]/3, 1, 1]})
        native.click_row(row('Write the note…', 'TextInput'))
        native.call('t', t='First note', wait=1)
        row('Editing', 'Label')
        checks.append('An unposted note is marked Editing')
        native.click_row(row('Post'))
        row('First note', 'Label')
        native.click_row(row('Reply…', 'TextInput'))
        native.call('t', t='Reply draft', wait=1)
        row('Reply draft', 'TextInput')

        right_click(row('First note', 'Label'))
        native.click_row(row('Delete Comment', 'PopupMenuItem'))
        row('Finish or discard the current edit before deleting a comment.', 'Label')
        row('Reply draft', 'TextInput')
        row('First note', 'Label')
        native.capture('reply-delete-guard')
        checks.append('Deleting a comment preserves its unfinished reply')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row('Reply draft', 'TextInput')
        native.click_row(row('Post'))
        row('Reply draft', 'Label')
        checks.append('The reply survives restyle and can be posted')
        key('KeyS', cmd=1)
        row('Saved', 'Label')
        key('KeyW', cmd=1)
        row('Recent', 'Label')
        time.sleep(.8)
        native.click_row(row('Comment form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Comment'))
        row('First note', 'Label')
        row('Reply draft', 'Label')
        checks.append('The saved note and reply reopen')

        native.click_row(row('Reply…', 'TextInput'))
        native.call('t', t='Another draft', wait=1)
        native.click_row(native.wait(lambda: native.find(identifier='more_pick', kind=None), timeout=30))
        native.click_row(row('Delete Comment', 'PopupMenuItem'))
        row('Finish or discard the current edit before deleting a comment.', 'Label')
        native.click_row(row('Another draft', 'TextInput'))
        key('Escape')
        row('Reply…', 'TextInput')
        native.click_row(native.wait(lambda: native.find(identifier='more_pick', kind=None), timeout=30))
        native.click_row(row('Delete Comment', 'PopupMenuItem'))
        native.wait(lambda: not native.find(text='First note', kind='Label'), timeout=30)
        checks.append('Toolbar deletion works after explicit discard')

        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'generated local PDF and synthetic author'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
