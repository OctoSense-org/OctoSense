#!/usr/bin/env python3
"""Exercise Calendar's add failure and retry on a disposable local store."""
import argparse
from datetime import date
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
    parser.add_argument('--action', choices=['new', 'duplicate'], required=True)
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
        '--test-action', 'launch-calendar'], out, 'calendar-storage')
    directory = state/'apps/.host/calendar'
    store = directory/'events.json'
    blocked = directory/('.events-' + str(native.child.pid) + '.json')
    checks = []

    def row(text=None, identifier=None, kind='Button'):
        return native.wait(lambda: native.find(text=text, identifier=identifier, kind=kind), timeout=30)

    def events():
        if not store.exists():
            return []
        try:
            return json.loads(store.read_text())
        except (OSError, ValueError):
            return None

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def action():
        if args.action == 'new':
            native.click_row(row(text='+ Event'))
        else:
            key('KeyD', cmd=1)

    try:
        row(text='+ Event')
        time.sleep(1)
        directory.mkdir(parents=True, exist_ok=True)
        if args.action == 'duplicate':
            native.click_row(row(text='+ Event'))
            row(identifier='e_title', kind='TextInput')
            native.wait(lambda: events() and len(events()) == 1, timeout=30)
            key('Escape')
            native.wait(lambda: not native.find(identifier='e_title', kind='TextInput'), timeout=30)
            time.sleep(1)
        original = events()
        assert original is not None
        blocked.mkdir()
        action()
        native.label("Couldn't Add Event")
        row(text='Try Again')
        assert events() == original, 'A blocked operation changed the event store'
        native.capture('add-failed')
        checks.append('A failed add explains the failure without changing events')

        # The sheet owns pointer and keyboard input until it is answered.
        native.click_row(row(text='+ Event'))
        key('KeyN', cmd=1)
        native.label("Couldn't Add Event")
        assert events() == original, 'Input behind the sheet changed events'
        checks.append('The failure sheet shields the calendar beneath it')
        key('Escape')
        native.wait(lambda: not native.find(text="Couldn't Add Event", kind='Label'), timeout=30)
        assert events() == original
        time.sleep(.3)
        action()
        native.label("Couldn't Add Event")
        checks.append('Escape cancels without changes and another attempt is possible')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        native.label("Couldn't Add Event")
        row(text='Try Again')
        native.click_row(row(text='Try Again'))
        time.sleep(.3)
        native.label("Couldn't Add Event")
        assert events() == original, 'A blocked retry changed events'
        checks.append('The sheet survives restyle and a repeated failure')

        blocked.rmdir()
        key('ReturnKey')
        native.wait(lambda: events() is not None and len(events()) == len(original) + 1, timeout=30)
        saved = events()
        if args.action == 'new':
            row(identifier='e_title', kind='TextInput')
            assert saved[0]['start'].startswith(date.today().isoformat() + 'T09:00'), saved[0]['start']
        else:
            assert len({event['id'] for event in saved}) == 2
            for field in ('title', 'start', 'end', 'location', 'notes', 'timezone'):
                assert saved[0].get(field) == saved[1].get(field), field
        checks.append('Return retries and adds exactly the intended event after storage recovers')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit')), \
            'Script error or execution limit; inspect ' + native.log_path.name
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'action': args.action,
            'checks': checks}, indent=2) + '\n')
        print('PASS:', args.appearance, args.action, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        if blocked.is_dir():
            blocked.rmdir()
        native.close()


if __name__ == '__main__':
    main()
