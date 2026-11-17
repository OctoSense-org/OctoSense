#!/usr/bin/env python3
"""Exercise event-edit recovery against a blocked disposable Calendar store."""
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
    parser.add_argument('--narrow', action='store_true')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-calendar'], out, 'calendar-edit-storage')
    directory = state/'apps/.host/calendar'
    store = directory/'events.json'
    blocked = directory/('.events-' + str(native.child.pid) + '.json')
    wanted = 'Changes awaiting storage'
    checks = []

    def row(text=None, identifier=None, kind='Button'):
        return native.wait(lambda: native.find(text=text, identifier=identifier, kind=kind), timeout=30)

    def title():
        return (native.find(identifier='e_title', kind='TextInput') or {}).get('t')

    def event():
        try:
            values = json.loads(store.read_text())
            return values[0] if len(values) == 1 else None
        except (OSError, ValueError):
            return None

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def change(value):
        native.click_row(row(identifier='e_title', kind='TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t=value, wait=1)
        native.wait(lambda: title() == value)
        key('ReturnKey')

    try:
        row(text='+ Event')
        time.sleep(1)
        if args.narrow:
            windows = [r for r in native.rows() if r.get('ty') in ('MpModuleView', 'MpRunView')]
            rect = max(windows, key=lambda r: r['r'][2] * r['r'][3])['r']
            x, y = rect[0] + rect[2] - 2, rect[1] + rect[3] * .5
            to = rect[0] + 700 - 2
            native.call('m', k='move', x=x, y=y, wait=1)
            native.call('m', k='down', x=x, y=y, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=x + (to - x) * step / 6, y=y, wait=1)
            native.call('m', k='up', x=to, y=y, wait=1)
            time.sleep(.8)
        native.click_row(row(text='+ Event'))
        row(identifier='e_title', kind='TextInput')
        native.wait(event, timeout=30)
        blocked.mkdir()
        change(wanted)
        retry = row(identifier='edit_retry')
        status = row(identifier='edit_status', kind='Label').get('t', '')
        assert 'storage' in status.lower() and 'os error' not in status.lower(), status
        assert event()['title'] != wanted
        native.capture('edit-write-failed')
        checks.append('Failure is explained without internal error details')

        key('Escape')
        assert title() == wanted and native.find(identifier='edit_retry', kind='Button')
        native.click_row(retry)
        time.sleep(.4)
        row(identifier='edit_retry')
        assert title() == wanted and event()['title'] != wanted
        checks.append('Escape and another failed retry keep the edited title')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        native.wait(lambda: title() == wanted, timeout=30)
        row(identifier='edit_retry')
        checks.append('The unsaved edit and retry survive an appearance switch')

        blocked.rmdir()
        native.click_row(row(identifier='edit_retry'))
        native.wait(lambda: event() and event()['title'] == wanted, timeout=30)
        native.wait(lambda: not native.find(identifier='edit_retry', kind='Button'), timeout=30)
        assert title() == wanted
        checks.append('Retry saves the same edited title after storage recovers')

        blocked.mkdir()
        change('A change to discard')
        row(identifier='edit_retry')
        native.click_row(row(text='Discard Changes'))
        native.wait(lambda: not native.find(identifier='e_title', kind='TextInput'), timeout=30)
        assert event()['title'] == wanted
        checks.append('Discard Changes closes without overwriting the saved event')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit')), \
            'Script error or execution limit; inspect ' + native.log_path.name
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'narrow': args.narrow,
            'checks': checks}, indent=2) + '\n')
        print('PASS:', args.appearance, 'narrow' if args.narrow else 'default', len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        if blocked.is_dir():
            blocked.rmdir()
        native.close()


if __name__ == '__main__':
    main()
