#!/usr/bin/env python3
"""Switch the live phone appearance with a calendar draft already being edited."""
import argparse
import json
import os
from pathlib import Path
import time
from capture import AppNative, ROOT


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.environ.update(OCTOSENSE_HOME=str(out/'home'), OCTOSENSE_APP_DATA=str(out/'apps'), OCTOS_APP_CORE_DIR=str(out/'core'))
    a = AppNative(args.binary.resolve(), ['--test-action', 'phone:ios', '--test-action', 'launch-calendar'], out, 'theme')
    title = 'Keep this unsaved draft'
    try:
        a.label('Calendar')
        time.sleep(1)
        a.click('+ Event')
        a.field('e_title', title)
        a.call('k', k='press', c='Escape', wait=1)
        time.sleep(.3)
        for mode in ['dark', 'light']:
            a.call('click', x=332, y=16, wait=1)
            time.sleep(1)
            row = a.find(identifier='e_title', kind='TextInput')
            assert row and row.get('t') == title, 'A theme change lost the visible unsaved editor'
            a.capture('draft-' + mode)
        a.click('Save')
        a.label(title)
        a.check_logs()
        (out/'receipt.json').write_text(json.dumps({'checks': ['Unsaved editor and text retained through dark and light switches', 'Same draft saved after both switches'], 'physical_mobile': False}, indent=2)+'\n')
        print('PASS: dark/light appearance switches retain the active editor and its unsaved draft')
    except Exception:
        a.capture('failed-state')
        raise
    finally:
        a.close()


if __name__ == '__main__':
    main()
