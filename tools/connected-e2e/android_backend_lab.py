#!/usr/bin/env python3
"""Prepare or deploy a fresh, signed Android backend-auth acceptance lab.

No logged-in data is injected. Registration is public loopback fixture metadata;
the person signs up/logs in on the synthetic backend's actual web form.
"""
import argparse
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tarfile
import urllib.parse
from android_backend import fixture_installation, sha
from backend_login import bundle, APP, OTHER

HERE = Path(__file__).resolve().parent


def prepare(args):
    args.out.mkdir(parents=True, mode=0o700, exist_ok=False)
    metadata = json.loads(args.server_metadata.read_text())
    registration = metadata['registration']
    parsed = urllib.parse.urlsplit(registration['authorization_url'])
    if parsed.scheme != 'http' or parsed.hostname != '127.0.0.1' or not parsed.port:
        raise ValueError('Expected the local synthetic server metadata')
    glance = (HERE / 'backend-login/glance.splash').read_text().rstrip('\n')
    for app in (APP, OTHER):
        path = args.out / app
        bundle(path, app, args.hub)
        manifest = json.loads((path / 'manifest.json').read_text())
        manifest['capabilities'].append('glance')
        (path / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        listing = json.loads((path / 'listing.json').read_text())
        listing['platforms'] = ['macos', 'android']
        (path / 'listing.json').write_text(json.dumps(listing, indent=2) + '\n')
        source = (path / 'main.splash').read_text()
        marker = 'status := Label'
        if source.count(marker) != 1:
            raise ValueError('Expected the unchanged fixture status control')
        source = source.replace(marker, 'Button {width: Fill height: 44 text: "Show login in Glance" on_click: || show_login_glance()}\n ' + marker)
        source = ('fn show_login_glance() {\n host.request("glance.publish", '
                  '{card_id: "backend-auth", title: "Backend login in Glance", '
                  'summary: "Synthetic native sign-in acceptance", script: ' + json.dumps(glance) +
                  ', viewport: true, notify: false}, fn(r) {\n'
                  '  if r.is_ok { notice("Login card published in Glance") }\n'
                  '  else { notice(r.error) }\n })\n}\n' + source)
        (path / 'main.splash').write_text(source)
        subprocess.run([str(args.hub), 'stamp', str(path)], check=True, capture_output=True)
    result = subprocess.run([str(args.installer), '--keep-profile=' + str(args.out / 'apps'),
                             str(args.out / APP), str(args.out / OTHER)], check=True, capture_output=True)
    (args.out / 'signed-install.json').write_bytes(result.stdout)
    specs = [dict(registration, app_id=app, id='fixture' if app == APP else 'fixture-other')
             for app in (APP, OTHER)]
    (args.out / 'backend-registrations.json').write_text(json.dumps(specs) + '\n')
    fixture_installation(args.out / 'apps')
    print(json.dumps({'prepared': True, 'signed_install': True, 'accounts_created': False}))


def deploy(args):
    if not re.fullmatch(r'dev\.makepad\.octosense\.backendlogin(?:\.[a-z0-9_]+)*', args.package):
        raise ValueError('Only the separate backendlogin package is allowed')
    receipt = json.loads((args.packaged / 'receipt.json').read_text())
    installed = fixture_installation(args.profile)
    apk = args.packaged / 'OctoSenseBackendLoginTest.apk'
    if (receipt['package'] != args.package or receipt['apk_sha256'] != sha(apk)
            or receipt['installation'] != installed or receipt['signature_verified'] is not True):
        raise ValueError('APK and signed profile receipt mismatch')
    base = [str(args.adb)] + (['-s', args.serial] if args.serial else [])

    def run(*values, data=None):
        return subprocess.run(base + list(values), input=data, check=True, capture_output=True).stdout

    probe = subprocess.run(base + ['shell', 'pm', 'path', args.package], capture_output=True)
    if probe.returncode not in (0, 1) or probe.stderr.strip():
        raise ValueError('Could not establish package absence')
    if probe.stdout.strip():
        raise ValueError('Refusing an existing package; choose a fresh test suffix')
    specs = json.loads(args.registrations.read_text())
    if len(specs) != 2 or {s['app_id'] for s in specs} != {APP, OTHER}:
        raise ValueError('Expected exactly two synthetic registrations')
    ports = set()
    for spec in specs:
        origin = urllib.parse.urlsplit(spec['authorization_url'])
        if origin.scheme != 'http' or origin.hostname != '127.0.0.1' or not origin.port:
            raise ValueError('Only the local synthetic server is allowed')
        ports.add(origin.port)
    if len(ports) != 1:
        raise ValueError('Both apps must use the same isolated synthetic server')
    # Reject links and special entries; never archive an unrelated profile.
    files = sorted(args.profile.rglob('*'))
    if any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in files):
        raise ValueError('Profile must contain only regular files and directories')
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w') as tar:
        for path in files:
            tar.add(path, arcname='apps/' + str(path.relative_to(args.profile)), recursive=False)
        data = json.dumps(specs).encode()
        entry = tarfile.TarInfo('apps/.host/fixtures/backend-registrations.json')
        entry.mode = 0o600
        entry.size = len(data)
        tar.addfile(entry, io.BytesIO(data))
    run('install', str(apk))
    run('shell', 'run-as', args.package, 'mkdir', '-p', 'files')
    run('shell', '-T', 'run-as', args.package, 'tar', '-xf', '-', '-C', 'files', data=archive.getvalue())
    port = next(iter(ports))
    run('reverse', 'tcp:' + str(port), 'tcp:' + str(port))
    # Android deliberately replaces MAKEPAD_APP_CONFIG from this one-shot
    # Intent extra; an environment value in wrap.sh alone is insufficient.
    config = json.dumps({'test_actions': ['backend-auth-fixture', 'launch-hub:' + APP]}, separators=(',', ':'))
    run('shell', 'am', 'start', '-n', args.package + '/.MakepadApp',
        '--es', 'makepad.APP_CONFIG', shlex.quote(config))
    print(json.dumps({'installed': True, 'fresh_package': True, 'signed_profile': True,
                      'real_login_required': True, 'credentials_injected': False}))


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest='action', required=True)
    prep = actions.add_parser('prepare')
    for key in ('hub', 'installer', 'server-metadata', 'out'):
        prep.add_argument('--' + key, type=Path, required=True)
    dep = actions.add_parser('deploy')
    for key in ('adb', 'packaged', 'profile', 'registrations'):
        dep.add_argument('--' + key, type=Path, required=True)
    dep.add_argument('--package', default='dev.makepad.octosense.backendlogin')
    dep.add_argument('--serial', help='Assigned test device only; never stored in receipts')
    args = parser.parse_args()
    (prepare if args.action == 'prepare' else deploy)(args)


if __name__ == '__main__':
    main()
