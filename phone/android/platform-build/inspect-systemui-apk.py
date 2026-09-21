#!/usr/bin/env python3
"""Inspect a native SystemUI build and compare its boundary to the phone's APK.

This never installs or bypasses signing checks. --require-installed-signer turns
an incompatible certificate into exit status 2, suitable for a packaging gate.
A packaging pass alone is not phone runtime or ROM-product validation.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import zipfile

spec = importlib.util.spec_from_file_location('manifest_parser', Path(__file__).with_name('inspect-quickstep-apk.py'))
parser_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parser_module)
PACKAGE = 'com.android.systemui'
OWN_ACTIVITY = PACKAGE + '.octosense.OctoSenseSystemActivity'


def normalized(name):
    return PACKAGE + name if name.startswith('.') else PACKAGE + '.' + name if '.' not in name else name


def compare(candidate, reference):
    errors = []
    def require(value, message):
        if not value: errors.append(message)
    require(candidate['attrs'].get('package') == PACKAGE, 'Package identity differs')
    require(candidate['attrs'].get('android:sharedUserId') == 'android.uid.systemui', 'Shared SystemUI identity differs')
    apps = [n for n in candidate['children'] if n['tag'] == 'application']
    old_apps = [n for n in reference['children'] if n['tag'] == 'application']
    require(len(apps) == 1 and len(old_apps) == 1, 'Expected one application per APK')
    if len(apps) != 1 or len(old_apps) != 1: return errors, {}
    app, old = apps[0], old_apps[0]
    for key in ['android:name','android:process','android:persistent','android:directBootAware',
                'android:defaultToDeviceProtectedStorage','android:allowClearUserData','android:appComponentFactory']:
        require(app['attrs'].get(key) == old['attrs'].get(key), 'Application boundary differs: ' + key)
    require(app['attrs'].get('android:persistent') == 0xffffffff, 'Persistent service process missing')
    requested = lambda tree: {n['attrs']['android:name'] for n in tree['children'] if n['tag'].startswith('uses-permission')}
    requested_new, requested_old = requested(candidate), requested(reference)
    require(requested_new == requested_old, 'Requested permissions differ from installed SystemUI')
    declared = lambda tree: {n['attrs']['android:name']:n['attrs'].get('android:protectionLevel') for n in tree['children'] if n['tag'] == 'permission'}
    require(declared(candidate) == declared(reference), 'Declared permission protection differs')
    components = lambda application: {(n['tag'],normalized(n['attrs']['android:name'])):n for n in application['children']
        if n['tag'] in ['activity','activity-alias','service','receiver','provider']}
    current, previous = components(app), components(old)
    for key, node in previous.items():
        require(key in current, 'Installed component missing: ' + key[1])
        if key not in current: continue
        for field in ['android:exported','android:permission','android:readPermission','android:writePermission',
                      'android:authorities','android:directBootAware','android:enabled','android:process']:
            require(current[key]['attrs'].get(field) == node['attrs'].get(field), 'Component boundary differs: ' + key[1] + '/' + field)
    added = set(current) - set(previous)
    require(added == {('activity',OWN_ACTIVITY)}, 'Unexpected added components: ' + repr(added))
    own = current.get(('activity',OWN_ACTIVITY), {'attrs':{},'children':[]})
    require(own['attrs'].get('android:exported') == 0, 'Device controls must not be exported')
    require(own['attrs'].get('android:showWhenLocked') == 0, 'Device controls must not show over keyguard')
    require(not own['children'], 'Device controls must not declare external intent entry points')
    require(any(n['tag'] == 'meta-data' and n['attrs'].get('android:name') == 'dev.makepad.octosense.SYSTEM_INTERFACE'
                and n['attrs'].get('android:value') == 1 for n in app['children']), 'OctoSense native interface marker missing')
    require(not any(n['tag'] == 'instrumentation' for n in candidate['children']), 'Test instrumentation must not ship')
    sdk = next((n['attrs'] for n in candidate['children'] if n['tag'] == 'uses-sdk'), {})
    require(sdk.get('android:targetSdkVersion') == 35, 'API 35 target required')
    require(isinstance(sdk.get('android:minSdkVersion'), int) and sdk['android:minSdkVersion'] <= 35,
            'Candidate must support the API 35 phone')
    return errors, {'existing_components_checked':len(previous),'requested_permissions':len(requested_new),
                    'extra_permissions':sorted(requested_new-requested_old),'missing_permissions':sorted(requested_old-requested_new)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk',type=Path,required=True)
    parser.add_argument('--installed-apk',type=Path,required=True)
    parser.add_argument('--aapt',required=True)
    parser.add_argument('--apksigner',required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--require-installed-signer',action='store_true')
    args = parser.parse_args()
    def read(apk):
        manifest = subprocess.check_output([args.aapt,'dump','xmltree',str(apk),'AndroidManifest.xml'],text=True)
        signature = subprocess.check_output([args.apksigner,'verify','--verbose','--print-certs',str(apk)],text=True)
        certs = re.findall(r'Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})',signature)
        assert len(certs) == 1, 'One verified signer required'
        return parser_module.manifest_tree(manifest), manifest, certs[0], signature
    candidate, manifest, cert, signature = read(args.apk)
    installed, old_manifest, old_cert, old_signature = read(args.installed_apk)
    failures, boundary = compare(candidate,installed)
    with zipfile.ZipFile(args.apk) as archive:
        dex = [p for p in archive.namelist() if re.fullmatch(r'classes\d*\.dex',p)]
        if not dex: failures.append('Native SystemUI DEX missing')
    compatible = cert == old_cert
    report = {'result':'fail' if failures else 'pass','failures':failures,'boundary':boundary,
        'apk_sha256':hashlib.sha256(args.apk.read_bytes()).hexdigest(),
        'installed_apk_sha256':hashlib.sha256(args.installed_apk.read_bytes()).hexdigest(),
        'certificate_sha256':cert,'installed_certificate_sha256':old_cert,
        'signer_matches_installed':compatible,'dex_files':dex,
        'deployment_blockers': ([] if compatible else ['installed_release_signer_mismatch']) + ['phone_runtime_validation_pending'],
        'phone_deployed':False,'phone_runtime_validated':False,'rom_product_image_built':False}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    args.output.with_suffix('.manifest.txt').write_text(manifest)
    args.output.with_suffix('.signer.txt').write_text(signature)
    print(json.dumps(report,indent=2))
    raise SystemExit(1 if failures else 2 if args.require_installed_signer and not compatible else 0)


if __name__ == '__main__': main()
