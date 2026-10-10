#!/usr/bin/env python3
"""Install and run the genuine demo apps in an isolated Mac desktop shell.

--app-zip tests the packaged release, extracted only into the new evidence
directory after checking the GitHub release's SHA256SUMS digest. --binary is
an explicitly recorded developer rehearsal; it never counts as release proof.
This driver uses App Hub's UI and synthetic local data, not a model or account.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/connected-e2e'))
from native import Native

APPS = [('org.ymote.componentdemo.first', 'Component Demo · First'),
        ('org.ymote.componentdemo.second', 'Component Demo · Second')]


def digest(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
    return result.hexdigest()


def unpack(archive, expected, tag, destination):
    if digest(archive) != expected:
        raise ValueError('Release archive does not match its SHA256SUMS digest')
    # ditto preserves the signed bundle layout. Reject escaping archive names
    # first; the input must be the official, checksum-verified app.zip asset.
    with zipfile.ZipFile(archive) as source:
        for item in source.infolist():
            path = Path(item.filename)
            if path.is_absolute() or '..' in path.parts or '\\' in item.filename:
                raise ValueError('Unsafe archive entry')
    subprocess.run(['ditto', '-x', '-k', str(archive), str(destination)], check=True)
    app = destination / 'OctoSense.app'
    with (app / 'Contents/Info.plist').open('rb') as stream:
        info = plistlib.load(stream)
    if info.get('CFBundleShortVersionString') != tag.removeprefix('desktop-v'):
        raise ValueError('Packaged version differs from the requested release tag')
    binary = app / 'Contents/MacOS/octosense'
    if binary.is_symlink() or not binary.is_file():
        raise ValueError('Expected a regular packaged OctoSense executable')
    return binary


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument('--app-zip', type=Path)
    source.add_argument('--binary', type=Path, help='Developer rehearsal only')
    parser.add_argument('--sha256', help='Official app.zip SHA256SUMS entry')
    parser.add_argument('--tag')
    parser.add_argument('--mirror', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This acceptance driver runs the macOS desktop only')
    if args.app_zip and (not re.fullmatch(r'[0-9a-f]{64}', args.sha256 or '') or
                         not re.fullmatch(r'desktop-v\d+\.\d+\.\d+(?:-[\w.]+)?', args.tag or '')):
        parser.error('--app-zip requires --sha256 and --tag')
    if args.binary and (args.sha256 or args.tag):
        parser.error('A developer binary cannot claim release provenance')
    mirror = args.mirror.resolve(strict=True)
    if not (mirror / 'catalog-v2.json').is_file():
        parser.error('Use the genuine GitHub-attested catalog mirror')
    out = args.out.resolve()
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    receipt = {'schema': 1, 'release_archive_tested': bool(args.app_zip),
               'tag': args.tag, 'archive_sha256': args.sha256,
               'catalog_sha256': digest(mirror / 'catalog-v2.json'),
               'checks': {}, 'personal_accounts_used': False,
               'live_model_tested': False, 'public_catalog_modified': False}
    ui = None

    def check(name, result):
        receipt['checks'][name] = bool(result)
        assert result, name

    try:
        binary = (unpack(args.app_zip.resolve(strict=True), args.sha256, args.tag, out / 'package')
                  if args.app_zip else args.binary.resolve(strict=True))
        receipt['binary_sha256'] = digest(binary)
        profile = out / 'profile'
        profile.mkdir(mode=0o700)
        # No inherited provider keys, personal vault overrides or test trust
        # anchor. HOME is preserved; every application data root is explicit.
        env = {key: os.environ[key] for key in ('PATH', 'HOME', 'TMPDIR', 'LANG') if key in os.environ}
        env.update(OCTOSENSE_HOME=str(profile), OCTOS_APP_CORE_DIR=str(out / 'kernel'),
                   OCTOSENSE_MAIL_VAULT='file', OCTOSENSE_LLM_VAULT='file', OCTOSENSE_SECRETS='file',
                   OCTOSENSE_HUB=str(mirror), OCTOSENSE_HUB_CATALOG='github', MAKEPAD_REMOTE='1')

        def start(name, app):
            return Native(binary, ['--test-action', 'launch-' + app], out, name, env=env, cwd=out)

        ui = start('hub-install', 'apphub')
        ui.wait(lambda: ui.find(identifier='search_tab'), timeout=60)
        ui.click_row(ui.reachable(identifier='search_tab'))
        for app_id, name in APPS:
            ui.field('search', name)
            ui.label(name)
            ui.click('Get')
            ui.label('Install ' + name + '?')
            ui.click('Install')
            ui.wait(lambda: any('Installed. Your app is ready to open.' in row.get('t', '')
                               for row in ui.rows()), timeout=90)
            check(app_id + '.installed_through_hub', True)
            ui.capture(app_id + '-installed')
            ui.click_row(ui.reachable(identifier='back'))
        ui.check_logs()
        ui.close()
        ui = None
        # A new shell each time proves the installation survives restart and
        # does not depend on the store's in-memory test fixture. Instance
        # concurrency and agent tool dispatch have their separate native test.
        for app_id, name in APPS:
            ui = start(app_id, 'hub:' + app_id)
            ui.wait(lambda: any(row.get('t') == name and row.get('ty') == 'Label'
                               for row in ui.rows()), timeout=60)
            for index, expected in enumerate(('1 → 2', '3 → 4'), 1):
                ui.click('Run checks')
                ui.label('Shared components answered')
                check(app_id + '.turn_' + str(index), bool(ui.label("This app's counter: " + expected)))
                check(app_id + '.private_note_' + str(index), bool(ui.label('<h1>' + name + '</h1>')))
            ui.call('m', k='move', x=20, y=100, wait=1)
            ui.capture(app_id + '-completed')
            ui.check_logs()
            ui.close()
            ui = None
        receipt['passed'] = True
    except Exception as error:
        receipt.update(passed=False, error=str(error))
        raise
    finally:
        if ui:
            ui.close()
        (out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(f"{sum(receipt['checks'].values())}/{len(receipt['checks'])} checks passed; receipt: {out / 'receipt.json'}")


if __name__ == '__main__':
    main()
