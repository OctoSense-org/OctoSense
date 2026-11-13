#!/usr/bin/env python3
"""Check Saved Places failure and retry with a disposable read-only file."""
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
    places = state/'apps/os.maps/accounts/device'
    places.mkdir(parents=True)
    saved = places/'saved.json'
    original = json.dumps([{'id': '', 'name': 'Fixture Cafe', 'cat': 'Cafe',
                            'label': 'Sample City', 'lat': 37.3321, 'lon': -121.8886}])
    saved.write_text(original)
    saved.chmod(0o444)
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-maps']
    native = AppNative(args.binary.resolve(), actions, out, 'maps-saved-storage')
    checks = []
    error = "Couldn't update Saved Places. Check storage and try again."

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def no_error():
        return not native.find(text=error, kind='Label')

    def clean_log():
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))

    try:
        row('Fixture Cafe', 'Label')
        time.sleep(.8)
        native.click_row(row('Fixture Cafe', 'Label'))
        row('Saved')
        native.click_row(row('Saved'))
        row(error, 'Label')
        assert saved.read_text() == original and native.find(text='Saved', kind='Button')
        checks.append('A refused removal explains the failure and keeps the saved place')
        native.capture('saved-write-refused')

        native.click_row(row('Saved'))
        row(error, 'Label')
        assert saved.read_text() == original and native.find(text='Saved', kind='Button')
        checks.append('A repeated refusal does not change the saved place')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row(error, 'Label')
        time.sleep(9)
        row(error, 'Label')
        row('Saved')
        checks.append('The failure notice survives appearance changes and waiting')

        saved.chmod(0o644)
        native.click_row(row('Saved'))
        native.wait(lambda: native.find(text='Save', kind='Button') and
                    json.loads(saved.read_text()) == [] and no_error(), timeout=30)
        checks.append('The same removal succeeds when storage is writable')

        saved.chmod(0o444)
        native.click_row(row('Save'))
        row(error, 'Label')
        assert json.loads(saved.read_text()) == [] and native.find(text='Save', kind='Button')
        native.click_row(row('Dismiss'))
        native.wait(no_error, timeout=15)
        assert json.loads(saved.read_text()) == [] and native.find(text='Save', kind='Button')
        checks.append('A failed addition stays unsaved when its notice is dismissed')

        saved.chmod(0o644)
        native.click_row(row('Save'))
        native.wait(lambda: native.find(text='Saved', kind='Button') and
                    len(json.loads(saved.read_text())) == 1, timeout=30)
        clean_log()
        native.close()
        native = AppNative(args.binary.resolve(), actions, out, 'maps-saved-reopen')
        place = native.wait(lambda: next((r for r in native.rows() if r.get('ty') == 'Label'
                                         and r.get('i') == 'name' and r.get('t') == 'Fixture Cafe'), None), timeout=60)
        native.click_row(place)
        row('Saved')
        assert len(json.loads(saved.read_text())) == 1
        checks.append('A successful addition survives reopening the app')
        native.capture('saved-after-reopen')
        clean_log()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'disposable read-only Saved Places file'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        saved.chmod(0o644)
        native.close()


if __name__ == '__main__':
    main()
