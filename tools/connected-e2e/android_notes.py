#!/usr/bin/env python3
"""Package an isolated Android Notes lab with the normal signed catalog gate.

The input APK must already be a separate, debuggable Home build. This adds
Android's supported wrap.sh startup environment, aligns and signs a new APK
using the existing Makepad development key. It does not install an APK, alter
a production catalog, create provider credentials or bypass app admission.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture_installation(profile):
    """Read only the public, typed receipt fields from a fresh installation."""
    host = profile / '.host'
    if host.exists() or host.is_symlink():
        raise ValueError('Expected a fresh installation without .host state')
    receipt = json.loads((profile / '.connected-e2e.json').read_text())
    if not isinstance(receipt, dict):
        raise ValueError('Expected the signed Notes fixture receipt')
    anchor = receipt.get('anchor')
    apps = receipt.get('apps')
    if (type(receipt.get('schema')) is not int or receipt['schema'] != 1
            or receipt.get('fixture') != 'connected-e2e'
            or not isinstance(anchor, str) or not re.fullmatch('[0-9a-f]{64}', anchor)
            or not isinstance(apps, list) or len(apps) != 1
            or not isinstance(apps[0], dict)):
        raise ValueError('Expected the exact signed Notes fixture installation')
    app = apps[0]
    digest = app.get('bundle_digest')
    if (app.get('id') != 'org.octosense.samples.githubnotes'
            or app.get('signed_install') is not True
            or app.get('prepared_launch_verified') is not True
            or not isinstance(digest, str) or not re.fullmatch('[0-9a-f]{64}', digest)
            or receipt.get('private_keys') != 'ephemeral memory only'
            or receipt.get('provider_credentials') != 'not created or copied'):
        raise ValueError('Expected the exact signed Notes fixture installation')
    # Do not copy arbitrary fields or strings from user-supplied metadata into
    # a receipt intended for review. Actual signatures are checked by the host.
    return {'schema': 1, 'fixture': 'connected-e2e', 'anchor': anchor,
            'apps': [{'id': 'org.octosense.samples.githubnotes',
                      'bundle_digest': digest, 'signed_install': True,
                      'prepared_launch_verified': True}],
            'private_keys': 'ephemeral memory only',
            'provider_credentials': 'not created or copied'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk', type=Path, required=True)
    parser.add_argument('--profile', type=Path, required=True,
                        help='Fresh signed profile made by connected-install')
    parser.add_argument('--build-tools', type=Path, required=True)
    parser.add_argument('--debug-keystore', type=Path, required=True)
    parser.add_argument('--package', default='dev.makepad.octosense.connectednotes')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'dev\.makepad\.octosense\.connectednotes(?:\.[a-z0-9_]+)*', args.package):
        parser.error('Use a separate connectednotes test package, never production Home')
    try:
        receipt = fixture_installation(args.profile)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    anchor = receipt['anchor']
    if args.out.exists() and any(args.out.iterdir()):
        parser.error('Output must be new or empty')
    args.out.mkdir(parents=True, mode=0o700, exist_ok=True)

    def run(*command):
        return subprocess.run([str(v) for v in command], check=True,
                              capture_output=True).stdout.decode()

    badging = run(args.build_tools / 'aapt2', 'dump', 'badging', args.apk)
    if f"name='{args.package}'" not in badging or 'application-debuggable' not in badging:
        parser.error('APK package must match and be debuggable')
    root = f'/data/user/0/{args.package}/files/apps'
    wrapper = ('#!/system/bin/sh\n'
               f'export OCTOSENSE_APP_DATA={root}\n'
               f'export OCTOSENSE_HUB={root}\n'
               f'export OCTOSENSE_HUB_ANCHOR={anchor}\n'
               'exec "$@"\n')
    (args.out / 'wrap.sh').write_text(wrapper)
    os.chmod(args.out / 'wrap.sh', 0o755)
    unaligned = args.out / 'unaligned.apk'
    with zipfile.ZipFile(args.apk) as source, zipfile.ZipFile(unaligned, 'w') as target:
        if 'lib/arm64-v8a/libmakepad.so' not in source.namelist():
            parser.error('Expected an arm64 Makepad APK')
        for entry in source.infolist():
            if entry.filename.startswith('META-INF/') or entry.filename == 'lib/arm64-v8a/wrap.sh':
                continue
            target.writestr(entry, source.read(entry.filename))
        entry = zipfile.ZipInfo('lib/arm64-v8a/wrap.sh')
        entry.external_attr = 0o100755 << 16
        entry.compress_type = zipfile.ZIP_DEFLATED
        target.writestr(entry, wrapper.encode())
    apk = args.out / 'OctoSenseNotesTest.apk'
    run(args.build_tools / 'zipalign', '-f', '-P', '16', '4', unaligned, apk)
    run(args.build_tools / 'apksigner', 'sign', '--ks', args.debug_keystore,
        '--ks-key-alias', 'androiddebugkey', '--ks-pass', 'pass:android',
        '--key-pass', 'pass:android', apk)
    run(args.build_tools / 'apksigner', 'verify', apk)
    unaligned.unlink()
    result = {'scope': 'separate debug Android Notes lab packaging',
              'package': args.package,
              'instrumentation': 'ADB (Makepad remote is unavailable on Android)',
              'input_apk_sha256': sha(args.apk), 'apk_sha256': sha(apk),
              'wrapper_sha256': sha(args.out / 'wrap.sh'),
              'installation': receipt, 'provider_credentials_copied': False,
              'signature_verified': True, 'device_install_verified': False}
    (args.out / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'apk': str(apk), 'sha256': result['apk_sha256'],
                      'signature_verified': True}))


if __name__ == '__main__':
    main()
