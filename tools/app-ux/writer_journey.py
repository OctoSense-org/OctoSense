#!/usr/bin/env python3
"""Exercise Writer's desktop Find and Replace with a disposable document."""
import argparse
import json
import os
from pathlib import Path
import time

from capture import AppNative, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()

    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-writer'], out, 'writer')

    def row(identifier, kind='Button'):
        return native.wait(lambda: native.find(identifier=identifier, kind=kind))

    def draft():
        return (native.find(identifier='editor_text', kind='DocumentEditor') or {}).get('t')

    def words(value):
        native.call('t', t=value, wait=1)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def field(identifier, value):
        native.click_row(row(identifier, 'TextInput'))
        key('KeyA', cmd=1)
        words(value)
        # If the editor stole focus, fail before accepting a changed document.
        observed = native.wait(lambda: {'text': draft()} if draft() is not None else None)
        assert observed['text'] == 'blue blue', 'Typing in Find changed the document: ' + repr(observed['text'])
        native.wait(lambda: (native.find(identifier=identifier, kind='TextInput') or {}).get('val') == value)

    def restyle(target):
        log_start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def style_reapplied():
            new_log = native.log_path.read_text()[log_start:]
            at = new_log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in new_log[at:]
        native.wait(style_reapplied, timeout=30)

    try:
        native.wait(lambda: len(native.rows()) > 4, timeout=45)
        time.sleep(1)
        for _ in range(3):
            deny = native.find(text="Don't allow")
            if not deny:
                break
            native.click_row(deny)
            time.sleep(.5)
        native.click_row(row('empty_new'))
        row('editor_text', 'DocumentEditor')
        words('red red')
        native.wait(lambda: draft() == 'red red')
        key('KeyF', cmd=1, alt=1)
        row('find_field', 'TextInput')
        words('red')
        native.label('1 of 2', identifier='find_count')
        native.click_row(row('replace_field', 'TextInput'))
        words('blue')
        native.click('Replace All')
        native.wait(lambda: draft() == 'blue blue')
        native.label('2 replaced', identifier='find_count')
        row('find_prev_off')
        row('find_next_off')
        assert not native.find(identifier='find_prev') and not native.find(identifier='find_next')
        time.sleep(.4)  # Let the word count and presented frame catch up for the capture.
        native.capture('replaced-all')

        # A restyle must keep the replacement field usable; switching to Find
        # must then leave the page untouched.
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        restyle(target)
        native.wait(lambda: draft() == 'blue blue')
        row('find_prev_off')
        row('find_next_off')
        words('!')
        native.wait(lambda: (native.find(identifier='replace_field', kind='TextInput') or {}).get('val') == 'blue!')
        assert draft() == 'blue blue', 'Typing after restyle changed the document'
        field('find_field', 'blue')
        native.wait(lambda: (native.find(identifier='find_count', kind='Label') or {}).get('t') in ('1 of 2', '2 of 2'))
        field('replace_field', 'bluebird')
        native.click('Replace All')
        native.wait(lambda: draft() == 'bluebird bluebird')
        native.label('2 replaced', identifier='find_count')
        native.click_row(row('find_next'))
        native.wait(lambda: (native.find(identifier='find_count', kind='Label') or {}).get('t') in ('1 of 2', '2 of 2'))
        time.sleep(1.2)  # Finish the appearance transition before capturing its pixels.
        native.capture('replacement-still-matches')
        # A click back on the page must also return typing to the document.
        native.click_row(row('editor_text', 'DocumentEditor'))
        words('!')
        native.wait(lambda: draft() is not None and '!' in draft())
        assert (native.find(identifier='find_field', kind='TextInput') or {}).get('val') == 'blue'
        key('KeyZ', cmd=1)
        native.wait(lambda: draft() == 'bluebird bluebird')
        # The page may hold focus while Find remains open; restyling must not
        # strand its keyboard just because the bar is visible.
        restyle(style)
        native.wait(lambda: draft() == 'bluebird bluebird')
        words('?')
        native.wait(lambda: draft() is not None and '?' in draft())
        assert (native.find(identifier='find_field', kind='TextInput') or {}).get('val') == 'blue'
        key('KeyZ', cmd=1)
        native.wait(lambda: draft() == 'bluebird bluebird')
        native.click('Done')
        key('KeyZ', cmd=1)
        native.wait(lambda: draft() == 'blue blue')
        native.check_logs()
        log = native.log_path.read_text()
        assert 'time budget' not in log and 'instruction limit' not in log, 'Script budget exceeded'
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': ['Replace All disables navigation when there are no matches',
                       'The replacement field keeps its input after restyle',
                       'Clicking Find after restyle leaves the document intact',
                       'A replacement that still matches remains navigable',
                       'Clicking the page returns typing to the document',
                       'Page typing stays available through a restyle with Find open',
                       'Replace All undoes in one step'], 'actions': native.actions}, indent=2) + '\n')
        print('PASS: Writer Find, replacement, focus after restyle and undo')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
