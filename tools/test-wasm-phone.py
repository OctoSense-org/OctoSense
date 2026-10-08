#!/usr/bin/env python3
"""Run the signed Host API Lab on an assigned Android phone in a fresh package.

Synthetic Wasm input only. No account, model, permission grant or production Home change.
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


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ('adb', 'apk', 'aapt2', 'hub', 'out'):
        parser.add_argument('--' + key, type=Path, required=True)
    parser.add_argument('--serial', help='Assigned phone only; omitted from evidence')
    parser.add_argument('--package', default='dev.makepad.octosense.hostapilab.wasm1')
    args = parser.parse_args()
    if not re.fullmatch(r'dev\.makepad\.octosense\.hostapilab(?:\.[a-z0-9_]+)*', args.package):
        parser.error('Use a separate hostapilab package, never Home')
    source_revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=ROOT):
        parser.error('Build and run the fixture from a clean committed source tree')
    runtime_tree = json.loads((ROOT / 'runtime-patches.lock.json').read_text())['makepad']['tree']
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
    shutil.copytree(ROOT / 'tools/fixtures/wasm-phone-lab/bundle', bundle)
    listing = json.loads((bundle / 'listing.json').read_text())
    listing['platforms'] = ['android']
    (bundle / 'listing.json').write_text(json.dumps(listing, indent=2) + '\n')
    subprocess.run([str(args.hub), 'stamp', str(bundle)], check=True, capture_output=True)
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w') as tar:
        for path in sorted(bundle.rglob('*')):
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise ValueError('Fixture accepts regular files and directories only')
            tar.add(path, arcname='wasm-phone-lab/bundle/' + str(path.relative_to(bundle)), recursive=False)
    installed = False
    receipt = {'schema': 1, 'platform': 'android', 'model': model, 'android': android,
               'package': args.package, 'apk_sha256': hashlib.sha256(args.apk.read_bytes()).hexdigest(),
               'personal_accounts_used': False, 'model_started': False, 'source_revision': source_revision, 'runtime_tree': runtime_tree, 'wasm_service_sha256': hashlib.sha256((ROOT/'crates/shell/src/wasm_service.rs').read_bytes()).hexdigest(), 'checks': {}}
    try:
        adb('install', str(args.apk))
        installed = True
        adb('shell', 'run-as', args.package, 'mkdir', '-p', 'files')
        adb('shell', '-T', 'run-as', args.package, 'tar', '-xf', '-', '-C', 'files', data=archive.getvalue())
        adb('shell', 'am', 'start', '-n', args.package + '/.MakepadApp')
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            result = adb('exec-out', 'run-as', args.package, 'cat',
                         'files/wasm-phone-lab/receipt.json', checked=False)
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

        require('compiled_source_identity_matches', native['source_revision'] == source_revision and native['source_dirty'] is False)
        require('compiled_runtime_identity_matches', native['runtime_tree'] == runtime_tree)
        require('signed_native_android_install', native['signed_install'] is True and native['platform'] == 'android')
        require('real_script_tool_completed', 'Ok' in native['tool_result'])
        data = native['tool_result']['Ok']
        require('bound_device_account', data['account'] == 'device')
        raw_results = data['results']
        require('raw_host_envelopes_forwarded', all(set(r) == {'is_ok','data','error'} for r in raw_results.values()) and set(data['functions']) == {'is_ok','data','error'})
        results = {name: {'is_ok': r['is_ok'], 'text': r['data']['text'] if r['is_ok'] else '', 'error': r['error']} for name, r in raw_results.items()}
        require('successful_guest_call', results['remember']['is_ok'] is True)
        require('success_does_not_retain_input', results['after_success']['is_ok'] is True and results['after_success']['text'] == '')
        require('guest_error_received', results['guest_error']['is_ok'] is False and 'failed' in results['guest_error']['error'])
        require('guest_error_does_not_retain_input', results['after_guest_error']['is_ok'] is True and results['after_guest_error']['text'] == '')
        require('guest_trap_received', results['trap']['is_ok'] is False and 'unreachable' in results['trap']['error'].lower())
        require('fresh_instance_after_trap', results['after_trap']['is_ok'] is True and results['after_trap']['text'] == '')
        require('guest_deadline_received', results['deadline']['is_ok'] is False and 'deadline' in results['deadline']['error'].lower())
        require('fresh_instance_after_deadline', results['after_deadline']['is_ok'] is True and results['after_deadline']['text'] == '')
        require('functions_answered', data['functions']['is_ok'] is True)
        modules = data['functions']['data']['modules']
        require('fresh_instance_policy_reported', len(modules) == 1 and modules[0]['instance_policy'] == 'fresh-per-call')
        require('eight_distinct_guest_invocations', modules[0]['invocations'] == 8)
        require('no_native_approval_sheet', native['host_sheet_visible'] is False)
        require('cross_account_refused', 'account_scope' in native['refusals']['wrong_account'])
        require('undeclared_tool_refused', bool(native['refusals']['undeclared_tool']))
        require('invalid_arguments_refused', bool(native['refusals']['invalid_arguments']))
        require('closed_tool_endpoint_refused', bool(native['closed_app']))
        receipt['passed'] = True
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
