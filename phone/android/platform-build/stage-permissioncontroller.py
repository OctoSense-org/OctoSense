#!/usr/bin/env python3
"""Stage the finite Settings adapters inside the pinned PermissionController.

The native role model, resources, permissions, and selection implementation are
unchanged. --check is read-only; --verify checks exact staged source bytes.
"""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

REVISION = '12d670229861f4ec3289418128589d751ac20c8c'
PREFIX = 'PermissionController/'


def once(text, before, after):
    if text.count(before) != 1:
        raise RuntimeError('Pinned PermissionController source mismatch: ' + before[:100])
    return text.replace(before, after, 1)


def generate(read, inputs, contract):
    components = (inputs / 'components.xml').read_text()
    root = ET.fromstring(components)
    if [node.tag for node in root] != ['service', 'activity', 'service', 'activity']:
        raise RuntimeError('Expected only role/permission services and native operation Activities')
    # Preserve the native manifest byte-for-byte apart from the four new nodes.
    nodes = components[components.index('>') + 1:components.rindex('</components>')]
    manifest = once(read('AndroidManifest.xml'), '    </application>', nodes + '\n    </application>')
    ET.fromstring(manifest)
    bp = once(read('Android.bp'), '        "src/**/*.kt",',
              '        "src/**/*.kt",\n        "src/com/android/permissioncontroller/octosense/IRoleSettings.aidl",\n        "src/com/android/permissioncontroller/octosense/IPermissionSettings.aidl",')
    changes = {'AndroidManifest.xml': manifest, 'Android.bp': bp}
    for source in sorted((inputs / 'files').rglob('*')):
        if source.is_file():
            path = str(source.relative_to(inputs / 'files'))
            if path in changes:
                raise RuntimeError('Duplicate adapter source: ' + path)
            changes[path] = source.read_text()
    changes['src/dev/makepad/octosense/roles/RolesSettingsContract.java'] = contract.read_text()
    permissions = contract.parent.parent / 'permissions/PermissionsSettingsContract.java'
    changes['src/dev/makepad/octosense/permissions/PermissionsSettingsContract.java'] = permissions.read_text()
    return changes


def stage(tree, inputs, contract, report, check=False, verify=False):
    repo = tree / 'packages/modules/Permission'
    def git(*parts):
        return subprocess.check_output(['git', '-C', str(repo), *parts], text=True)
    if git('rev-parse', 'HEAD').strip() != REVISION:
        raise RuntimeError('Permission module revision differs')
    changes = generate(lambda path: git('show', 'HEAD:' + PREFIX + path), inputs, contract)
    # The only accepted predecessor is our exact roles-only extension of this
    # same pinned revision. Never accept arbitrary edited manifest/build files.
    components = (inputs / 'components.xml').read_text()
    role_nodes = components[components.index('>') + 1:components.index('    <service android:name="com.android.permissioncontroller.octosense.OctoSensePermissionsService"')]
    predecessor = {
        'AndroidManifest.xml': once(git('show', 'HEAD:' + PREFIX + 'AndroidManifest.xml'),
                                    '    </application>', role_nodes + '\n    </application>'),
        'Android.bp': once(git('show', 'HEAD:' + PREFIX + 'Android.bp'), '        "src/**/*.kt",',
                           '        "src/**/*.kt",\n        "src/com/android/permissioncontroller/octosense/IRoleSettings.aidl",'),
    }
    original = {}
    for path in changes:
        result = subprocess.run(['git', '-C', str(repo), 'show', 'HEAD:' + PREFIX + path],
                                capture_output=True, text=True)
        original[path] = result.stdout if result.returncode == 0 else None
    dirty = set(git('diff', 'HEAD', '--name-only').splitlines()) | set(git('ls-files', '--others', '--exclude-standard').splitlines())
    if not dirty <= {PREFIX + path for path in changes}:
        raise RuntimeError('Preserve unrelated Permission changes: ' + repr(dirty))
    inspected = {}
    for path, expected in changes.items():
        file = repo / PREFIX / path
        actual = file.read_text() if file.exists() else None
        inspected[path] = actual
        if verify and actual != expected:
            raise RuntimeError('Staged source differs: ' + path)
        if not verify and actual not in (original[path], expected, predecessor.get(path)):
            raise RuntimeError('Preserve modified Permission source: ' + path)
    if not check and not verify:
        with (tree / '.octosense-build.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for path, actual in inspected.items():
                file = repo / PREFIX / path
                if (file.read_text() if file.exists() else None) != actual:
                    raise RuntimeError('Source changed after inspection: ' + path)
            for path, expected in changes.items():
                file = repo / PREFIX / path
                file.parent.mkdir(parents=True, exist_ok=True)
                if not file.exists() or file.read_text() != expected:
                    file.write_text(expected)
    record = {'status': 'pass', 'mode': 'check' if check else 'verify' if verify else 'stage',
              'permission_revision': REVISION, 'target': 'PermissionController',
              'package': 'com.android.permissioncontroller', 'device_deployed': False,
              'files': {PREFIX + p: hashlib.sha256(value.encode()).hexdigest() for p, value in changes.items()},
              'original_files': {PREFIX + p: hashlib.sha256(value.encode()).hexdigest() if value is not None else None for p, value in original.items()}}
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(record, indent=2) + '\n')
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tree', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--check', action='store_true')
    modes.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    record = stage(args.tree.resolve(), Path(__file__).resolve().parent / 'permissioncontroller',
                   root / 'vendor/octosense/settings/src/dev/makepad/octosense/roles/RolesSettingsContract.java',
                   args.report, args.check, args.verify)
    print(json.dumps({'status': record['status'], 'mode': record['mode'], 'files': len(record['files']), 'target': record['target']}))


if __name__ == '__main__':
    main()
