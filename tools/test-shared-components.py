#!/usr/bin/env python3
"""Run real GitHub shared-component acceptance in a hidden Mac host or fresh Android test package.

The mirror must contain an authentic catalog-v2.json and its exact candidate
artifacts. No legacy catalog, signing keys, account data, or model is used.
Run desktop first. Android requires an explicitly selected OnePlus 6.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument('--host', type=Path)
    target.add_argument('--apk', type=Path)
    parser.add_argument('--mirror', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--adb', type=Path)
    parser.add_argument('--aapt2', type=Path)
    parser.add_argument('--serial', help='Explicitly assigned OnePlus 6; never written to evidence')
    parser.add_argument('--package', default='dev.makepad.octosense.hostapilab.shared1')
    args = parser.parse_args()
    if not re.fullmatch(r'dev\.makepad\.octosense\.hostapilab\.[a-z0-9_]+', args.package):
        parser.error('Use a fresh hostapilab package, never Home')
    if args.apk and not all((args.adb, args.aapt2, args.serial)):
        parser.error('Android requires --adb, --aapt2 and an explicit --serial')
    if os.environ.get('OCTOSENSE_HUB_CATALOG', 'github') != 'github':
        parser.error('This test refuses the legacy catalog channel')
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=ROOT):
        parser.error('Use a clean committed source tree for source-bound evidence')
    runtime = json.loads((ROOT / 'runtime-patches.lock.json').read_text())['makepad']['tree']
    mirror = args.mirror.resolve(strict=True)
    if not (mirror / 'catalog-v2.json').is_file():
        parser.error('Mirror needs a real GitHub-attested catalog-v2.json')
    files = [p for p in mirror.rglob('*') if not p.is_dir()]
    if any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in mirror.rglob('*')):
        parser.error('Mirror accepts only regular files and directories')
    if sum(p.stat().st_size for p in files) > 32 * 1024 * 1024:
        parser.error('Synthetic mirror exceeds the 32 MiB acceptance limit')
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    receipt = {'schema': 1, 'source_revision': source, 'runtime_tree': runtime,
               'mirror_sha256': {str(p.relative_to(mirror)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)},
               'checks': {}, 'phone_used': bool(args.apk), 'personal_accounts_used': False,
               'public_catalog_modified': False}
    native = None
    ui = None
    installed = False
    adb_base = [str(args.adb), '-s', args.serial] if args.apk else None

    def adb(*values, checked=True, data=None):
        return subprocess.run(adb_base + list(values), input=data, capture_output=True, check=checked, timeout=60)

    def check(name, condition):
        receipt['checks'][name] = bool(condition)
        assert condition, name

    try:
        if args.host:
            sys.path.insert(0, str(ROOT / 'tools/connected-e2e'))
            from native import Native
            receipt['binary_sha256'] = hashlib.sha256(args.host.read_bytes()).hexdigest()
            # Ephemeral instrument port and hidden window are process-scoped.
            os.environ['MAKEPAD_REMOTE'] = '1'
            ui = Native(args.host.resolve(), ['--shared-mirror=' + str(mirror),
                        '--shared-home=' + str(args.out.resolve() / 'home'),
                        '--receipt=' + str(args.out.resolve() / 'native-result.json')], args.out, 'shared-components')
            deadline = time.monotonic() + 120
            while time.monotonic() < deadline:
                path = args.out / 'native-result.json'
                if path.exists():
                    native = json.loads(path.read_text())
                    break
                if ui.child.poll() is not None:
                    raise RuntimeError('Native host exited before its receipt')
                time.sleep(.15)
            if native is None:
                raise TimeoutError('Native shared-component host produced no receipt')
            # Force a settled present; the first hidden capture can precede
            # the completed glyph frame even after the widget tree exists.
            ui.call('m', k='move', x=20, y=100, wait=1)
            ui.capture('completed')
            ui.check_logs()
        else:
            check('assigned_oneplus6', adb('shell', 'getprop', 'ro.product.model').stdout.decode().strip()
                  in {'ONEPLUS A6000', 'ONEPLUS A6003', 'OnePlus 6'})
            present = adb('shell', 'pm', 'path', args.package, checked=False)
            check('test_package_absent', present.returncode in (0, 1) and not present.stdout.strip() and not present.stderr.strip())
            badging = subprocess.check_output([str(args.aapt2), 'dump', 'badging', str(args.apk)], text=True)
            check('isolated_debuggable_apk', f"name='{args.package}'" in badging and 'application-debuggable' in badging)
            receipt['apk_sha256'] = hashlib.sha256(args.apk.read_bytes()).hexdigest()
            receipt['package'] = args.package
            archive = io.BytesIO()
            with tarfile.open(fileobj=archive, mode='w') as tar:
                for path in sorted(mirror.rglob('*')):
                    tar.add(path, arcname='shared-components-lab/mirror/' + str(path.relative_to(mirror)), recursive=False)
            adb('install', str(args.apk))
            installed = True
            adb('shell', 'run-as', args.package, 'mkdir', '-p', 'files')
            adb('shell', '-T', 'run-as', args.package, 'tar', '-xf', '-', '-C', 'files', data=archive.getvalue())
            adb('shell', 'am', 'start', '-n', args.package + '/.MakepadApp')
            deadline = time.monotonic() + 150
            while time.monotonic() < deadline:
                result = adb('exec-out', 'run-as', args.package, 'cat', 'files/shared-components-lab/receipt.json', checked=False)
                if result.returncode == 0:
                    try:
                        native = json.loads(result.stdout)
                        break
                    except ValueError:
                        pass
                time.sleep(.25)
            if native is None:
                raise TimeoutError('Phone shared-component host produced no receipt')
            (args.out / 'native-result.json').write_text(json.dumps(native, indent=2) + '\n')
        check('native_platform', native['platform'] == ('android' if args.apk else 'macos'))
        check('compiled_source_identity', native['source_revision'] == source and native['source_dirty'] is False)
        check('compiled_runtime_identity', native['runtime_tree'] == runtime)
        check('real_github_trust', native['catalog_channel'] == 'github' and native['legacy_fallback'] is False)
        check('deduplicated_readonly_components', native['deduplicated_readonly_components'] is True)
        check('empty_first_declarations', native['apps'][0]['declarations'] in (None, []))
        check('populated_second_declarations', set(native['apps'][1]['declarations']) == {'wasm', 'storage'})
        check('all_four_native_turns', len(native['turns']) == 4)
        check('all_28_native_assertions', len(native['checks']) == 28 and all(native['checks'].values()))
        check('native_passed', native['passed'] is True)
        receipt['passed'] = True
    except Exception as error:
        receipt['passed'] = False
        receipt['error'] = str(error)
        raise
    finally:
        if ui:
            ui.close()
        if installed:
            adb('shell', 'am', 'force-stop', args.package, checked=False)
            # This invocation proved the package absent before installing it.
            removed = adb('uninstall', args.package, checked=False)
            receipt['owned_test_package_removed'] = removed.returncode == 0 and b'Success' in removed.stdout
        with (args.out / 'receipt.json').open('x') as output:
            json.dump(receipt, output, indent=2)
            output.write('\n')
        print(f"{sum(receipt['checks'].values())}/{len(receipt['checks'])} checks passed; receipt: {args.out / 'receipt.json'}")


if __name__ == '__main__':
    main()
