#!/usr/bin/env python3
"""Drive the owned charts-host example over Makepad's hidden native instrument."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/connected-e2e'))
from native import Native


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    profile = args.out / 'profile'
    profile.mkdir()
    env = {k: v for k, v in os.environ.items() if k in ('PATH', 'HOME', 'TMPDIR', 'LANG')}
    env.update(OCTOSENSE_HOME=str(profile), OCTOSENSE_SECRETS='file', MAKEPAD_REMOTE='0',
               XDG_CONFIG_HOME=str(profile / 'config'), XDG_DATA_HOME=str(profile / 'data'),
               XDG_CACHE_HOME=str(profile / 'cache'))
    digest = hashlib.sha256()
    with binary.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    receipt = {'schema': 1, 'binary_sha256': digest.hexdigest(), 'released_host_tested': False,
               'personal_accounts_used': False, 'render_settle_seconds': 3,
               'checks': {}, 'passed': False}
    app = None
    try:
        app = Native(binary, [], args.out, 'charts', env=env, cwd=str(ROOT))
        app.label('Select a mark')
        # The widget snapshot precedes asynchronous native font/vector uploads.
        # This explicit observation delay does not retry errors or extend UI waits.
        time.sleep(receipt['render_settle_seconds'])
        widgets = {}
        for name in ('line', 'bars', 'heat'):
            row = app.wait(lambda: app.find(identifier=name, kind=None))
            assert row['r'][2] > 200 and row['r'][3] > 180, 'Native chart has no useful draw area'
            widgets[name] = row
            receipt['checks'][name + '_draw_area'] = True
        app.capture('charts-initial')
        for name, label in [('line', 'Line point 1'), ('bars', 'Bar 1')]:
            x, y, width, height = widgets[name]['r']
            app.call('click', x=x + 46 + (width - 58) / 2, y=y + 12 + (height - 42) / 2, wait=1)
            app.label(label)
            receipt['checks'][name + '_click'] = True
        x, y, width, height = widgets['heat']['r']
        app.call('click', x=x + 12 + (width - 24) * 5 / 6, y=y + 12 + (height - 24) * 3 / 4, wait=1)
        app.label('Cell 1,2')
        receipt['checks']['heat_click'] = True
        app.capture('charts-interacted')
        app.click('Replace data')
        row = app.label('Data replaced:')
        arrays = row['t'].removeprefix('Data replaced: ').split(' / ')
        assert len(arrays) == 2 and all(json.loads(value) == [4, 8, 6] for value in arrays)
        receipt['checks']['line_and_bar_set_data_readback'] = True
        app.capture('charts-updated')
        app.check_logs()
        receipt['checks']['no_native_script_errors'] = True
        receipt['actions'] = app.actions
        receipt['passed'] = True
    except Exception as error:
        receipt['error'] = str(error)
        raise
    finally:
        if app:
            receipt['actions'] = app.actions
            app.close()
            receipt['owned_process_exited'] = app.child.poll() is not None
        (args.out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'passed': receipt['passed'], 'checks': len(receipt['checks'])}))


if __name__ == '__main__':
    main()
