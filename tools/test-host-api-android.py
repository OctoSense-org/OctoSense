#!/usr/bin/env python3
"""Run the signed Host API Lab on an assigned Android phone in a fresh package.

No account, model, permission grant, camera capture or production Home change.
The native host uses the same app-tool/host-service code as the desktop lab.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import time

ROOT = Path(__file__).resolve().parents[1]
OS_BATCH_CHECKS = (
    "files_status_discovery",
    "files_import_discovery",
    "files_export_discovery",
    "storage_binary_write_discovery",
    "contained_binary_roundtrip",
    "files_status_truthful",
    "location_sample_discovery",
    "background_import_refused",
    "background_export_refused",
    "background_location_sample_refused",
)


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('adb', 'apk', 'aapt2', 'hub', 'out'):
        parser.add_argument('--' + key, type=Path, required=True)
    parser.add_argument('--serial', help='Assigned phone only; omitted from evidence')
    parser.add_argument('--package', default='dev.makepad.octosense.hostapilab')
    args = parser.parse_args()
    if not re.fullmatch(r'dev\.makepad\.octosense\.hostapilab(?:\.[a-z0-9_]+)*', args.package):
        parser.error('Use a separate hostapilab package, never Home')
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    base = [str(args.adb)] + (['-s', args.serial] if args.serial else [])

    def adb(*values, data=None, checked=True):
        return subprocess.run(base + list(values), input=data, check=checked,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45)

    devices = adb('devices').stdout.decode().splitlines()[1:]
    if not args.serial and sum(line.endswith('\tdevice') for line in devices) != 1:
        parser.error('Select the assigned phone with --serial')
    model = adb('shell', 'getprop', 'ro.product.model').stdout.decode().strip()
    android = adb('shell', 'getprop', 'ro.build.version.release').stdout.decode().strip()
    present = adb('shell', 'pm', 'path', args.package, checked=False)
    if present.returncode not in (0, 1) or present.stderr.strip() or present.stdout.strip():
        parser.error('Expected a fresh, absent package; select a new suffix')
    badging = subprocess.run([str(args.aapt2), 'dump', 'badging', str(args.apk)],
                             check=True, capture_output=True).stdout.decode()
    if f"name='{args.package}'" not in badging or 'application-debuggable' not in badging:
        parser.error('APK must match the separate debuggable package')
    bundle = args.out / 'bundle'
    shutil.copytree(ROOT / 'tools/fixtures/host-api-lab/bundle', bundle)
    listing = json.loads((bundle / 'listing.json').read_text())
    listing['platforms'] = ['android']
    (bundle / 'listing.json').write_text(json.dumps(listing, indent=2) + '\n')
    subprocess.run([str(args.hub), 'stamp', str(bundle)], check=True, capture_output=True)
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w') as tar:
        for path in sorted(bundle.rglob('*')):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError('Fixture accepts regular files and directories only')
            tar.add(path, arcname='host-api-lab/bundle/' + str(path.relative_to(bundle)), recursive=False)
    installed = False
    receipt = {'schema': 1, 'platform': 'android', 'model': model, 'android': android,
               'package': args.package, 'apk_sha256': hashlib.sha256(args.apk.read_bytes()).hexdigest(),
               'personal_accounts_used': False, 'model_started': False, 'checks': {}}
    try:
        adb('install', str(args.apk))
        installed = True
        adb('shell', 'run-as', args.package, 'mkdir', '-p', 'files')
        adb('shell', '-T', 'run-as', args.package, 'tar', '-xf', '-', '-C', 'files', data=archive.getvalue())
        adb('shell', 'am', 'start', '-n', args.package + '/.MakepadApp')
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            result = adb('exec-out', 'run-as', args.package, 'cat',
                         'files/host-api-lab/receipt.json', checked=False)
            if result.returncode == 0:
                try:
                    native = json.loads(result.stdout)
                    break
                except ValueError:
                    pass
            time.sleep(.25)
        else:
            raise AssertionError('Native phone host did not produce a receipt')
        (args.out / 'native-result.json').write_text(json.dumps(native, indent=2) + '\n')
        checks = receipt['checks']

        def require(name, condition):
            checks[name] = bool(condition)
            if not condition:
                raise AssertionError(name)

        require('signed_native_android_install', native['signed_install'] is True and native['platform'] == 'android')
        require('real_script_tool_completed', 'Ok' in native['tool_result'])
        data = native['tool_result']['Ok']
        require('bound_device_account', data['account'] == 'device')
        require('native_camera_status', data['camera']['supported'] is True and data['camera']['os_permission'] in ('granted', 'not_determined', 'denied', 'settings_required'))
        require('declared_capability_is_not_consent', data['camera']['app_policy_granted'] is True and data['camera']['app_consent'] is False)
        require('runtime_api_discovery', data['discovery']['supported'] is True and data['discovery']['descriptor']['name'] == 'camera.permission.status')
        require('uncompiled_rust_function_refused', data['missing']['implemented'] is False and data['missing']['supported'] is False)
        require('undeclared_microphone_refused', data['microphone_allowed'] is False and data['microphone_error'])
        require('background_permission_prompt_refused', data['background_prompt_allowed'] is False and 'background' in data['background_error'])
        require('no_native_approval_sheet', native['host_sheet_visible'] is False)
        require('cross_account_refused', 'account_scope' in native['refusals']['wrong_account'])
        require('undeclared_tool_refused', bool(native['refusals']['undeclared_tool']))
        require('invalid_arguments_refused', bool(native['refusals']['invalid_arguments']))
        require('closed_tool_endpoint_refused', bool(native['closed_app']))
        batch = native.get('checks', {})
        if set(batch) != set(OS_BATCH_CHECKS):
            raise AssertionError('Incomplete native OS API batch checks')
        for name in OS_BATCH_CHECKS:
            require(name, batch[name] is True)
        receipt['passed'] = all(checks.values())
        receipt['not_verified'] = native['not_verified']
    except Exception as error:
        receipt['passed'] = False
        # Subprocess exceptions include their command (possibly --serial and
        # private build paths). Only our fixed assertion messages are public.
        receipt['error'] = str(error) if isinstance(error, AssertionError) else type(error).__name__
        raise
    finally:
        if installed:
            try:
                receipt['owned_package_stopped'] = adb(
                    'shell', 'am', 'force-stop', args.package, checked=False).returncode == 0
            except subprocess.SubprocessError:
                receipt['owned_package_stopped'] = False
        (args.out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({'passed': receipt['passed'], 'checks': len(receipt['checks']),
                      'owned_package_stopped': receipt['owned_package_stopped']}))


if __name__ == '__main__':
    main()
