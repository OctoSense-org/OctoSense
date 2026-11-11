#!/usr/bin/env python3
"""Check Calendar Undo and Redo after disposable storage failures."""
import argparse
import json
import os
from pathlib import Path
import time

from calendar_journey import CalendarNative, open_on_desktop
from capture import ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    parser.add_argument('--scenario', choices=['add', 'edit'], required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = CalendarNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-calendar'], out, 'calendar-history-storage')
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

    def change(value):
        native.click_row(row(identifier='e_title', kind='TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t=value, wait=1)
        key('ReturnKey')
        native.wait(lambda: events() and events()[0]['title'] == value, timeout=30)

    def close_editor():
        key('Escape')
        native.wait(lambda: not native.find(identifier='e_title', kind='TextInput'), timeout=30)
        time.sleep(.5)

    def undo():
        key('KeyZ', cmd=1)

    def redo():
        key('KeyZ', cmd=1, shift=1)

    try:
        row(text='+ Event')
        time.sleep(1)
        native.click_row(row(text='+ Event'))
        row(identifier='e_title', kind='TextInput')
        native.wait(lambda: events() and len(events()) == 1, timeout=30)
        if args.scenario == 'edit':
            change('Original event')
            close_editor()
            open_on_desktop(native, 'Original event')
            change('Changed event')
        close_editor()
        original = events()
        assert original and len(original) == 1

        blocked.mkdir()
        undo()
        native.label("Couldn't Undo")
        assert events() == original
        native.capture('undo-write-failed')
        checks.append('A storage failure is explained without claiming a conflict')
        native.click_row(row(text='Cancel'))
        native.wait(lambda: not native.find(text="Couldn't Undo", kind='Label'), timeout=30)
        time.sleep(.3)
        undo()
        native.label("Couldn't Undo")
        assert events() == original
        checks.append('Cancel keeps the Undo step available')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        native.label("Couldn't Undo")
        # Test the shield with storage writable, so an accidental New action
        # cannot appear to pass merely because the store rejected it.
        blocked.rmdir()
        native.click_row(row(text='+ Event'))
        key('KeyN', cmd=1)
        time.sleep(.5)
        native.label("Couldn't Undo")
        assert events() == original, 'Input reached the calendar behind the sheet'
        blocked.mkdir()
        key('Escape')
        native.wait(lambda: not native.find(text="Couldn't Undo", kind='Label'), timeout=30)
        time.sleep(.3)
        undo()
        native.label("Couldn't Undo")
        assert events() == original
        checks.append('After restyle the sheet shields shortcuts and Escape keeps Undo available')
        native.click_row(row(text='Try Again'))
        time.sleep(.3)
        native.label("Couldn't Undo")
        assert events() == original
        checks.append('Restyle and a failed retry preserve the Undo step')

        blocked.rmdir()
        key('ReturnKey')
        if args.scenario == 'add':
            native.wait(lambda: events() == [], timeout=30)
        else:
            native.wait(lambda: events() and events()[0]['title'] == 'Original event', timeout=30)
        assert not native.find(identifier='e_title', kind='TextInput'), 'Return opened the selected event'
        checks.append('Retry performs the intended Undo after recovery')
        time.sleep(.6)

        blocked.mkdir()
        redo()
        native.label("Couldn't Redo")
        native.capture('redo-write-failed')
        after_undo = events()
        native.click_row(row(text='Cancel'))
        native.wait(lambda: not native.find(text="Couldn't Redo", kind='Label'), timeout=30)
        time.sleep(.3)
        redo()
        native.label("Couldn't Redo")
        assert events() == after_undo
        checks.append('A blocked Redo can also be cancelled and retried')
        blocked.rmdir()
        native.click_row(row(text='Try Again'))
        if args.scenario == 'add':
            native.wait(lambda: events() and len(events()) == 1, timeout=30)
            restored = events()[0]
            for field in ('title', 'start', 'end', 'location', 'notes', 'timezone'):
                assert restored.get(field) == original[0].get(field), field
        else:
            native.wait(lambda: events() and events()[0]['title'] == 'Changed event', timeout=30)
        checks.append('Retry performs the intended Redo after recovery')

        # The retained alert must not reappear on another appearance change,
        # and keyboard history should remain usable after the recovered step.
        target = 'macos-dark' if args.appearance == 'dark' else 'macos'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        native.wait(restyled, timeout=30)
        assert not native.find(text="Couldn't Undo", kind='Label')
        assert not native.find(text="Couldn't Redo", kind='Label')
        time.sleep(1)
        undo()
        if args.scenario == 'add':
            native.wait(lambda: events() == [], timeout=30)
        else:
            native.wait(lambda: events() and events()[0]['title'] == 'Original event', timeout=30)
        checks.append('A later restyle leaves the recovered history available')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit')), \
            'Script error or execution limit; inspect ' + native.log_path.name
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'scenario': args.scenario,
            'checks': checks}, indent=2) + '\n')
        print('PASS:', args.appearance, args.scenario, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        if blocked.is_dir():
            blocked.rmdir()
        native.close()


if __name__ == '__main__':
    main()
