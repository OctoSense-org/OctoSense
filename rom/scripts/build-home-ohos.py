#!/usr/bin/env python3
"""Build a signed Home HAP using an existing DevEco SDK and signing profile."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def read_json5(path):
    # Preserve quoted strings while removing comments and trailing commas from
    # the generated DevEco templates. Signing input itself is ordinary JSON.
    token = re.compile(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/|,\s*(?=[}\]])')
    return json.loads(token.sub(lambda m: m.group() if m.group().startswith('"') else '', path.read_text()))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--deveco-home', required=True, type=Path)
    p.add_argument('--signing-config', required=True, type=Path,
                   help='External JSON array of existing DevEco signingConfigs; never printed')
    p.add_argument('--bundle-id', default='dev.makepad.octosense')
    p.add_argument('--packager', required=True, type=Path)
    p.add_argument('--compatible-sdk', default='6.0.1(21)')
    p.add_argument('--remote-port', type=int, help='Validation only: app-owned loopback inspection')
    p.add_argument('--offline', action='store_true')
    args = p.parse_args()
    for name in ('deveco_home', 'signing_config', 'packager'):
        setattr(args, name, getattr(args, name).expanduser().resolve())
    if args.remote_port is not None and not 1024 <= args.remote_port <= 65535:
        p.error('--remote-port must be between 1024 and 65535')
    if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+', args.bundle_id):
        p.error('Invalid bundle ID')
    if args.signing_config.is_relative_to(ROOT):
        p.error('Keep signing configuration outside the checkout')
    configs = json.loads(args.signing_config.read_text())
    default = next(c for c in configs if c['name'] == 'default')
    material = default['material']
    for name in ('profile', 'certpath', 'storeFile'):
        path = Path(material[name]).expanduser().resolve()
        if not path.is_file() or path.is_relative_to(ROOT):
            p.error('Signing material must exist outside the checkout')
        material[name] = str(path)
    # A profile authorizes one bundle ID. Do not silently substitute another
    # installed application's identity to make a signing failure disappear.
    profile = subprocess.run(['openssl', 'cms', '-verify', '-inform', 'DER', '-in', material['profile'],
                              '-noverify'], check=True, capture_output=True).stdout
    if json.loads(profile)['bundle-info']['bundle-name'] != args.bundle_id:
        p.error('Signing profile does not authorize --bundle-id')
    sdk = args.deveco_home / 'sdk/default/openharmony'
    cmake = sdk / 'native/build-tools/cmake/bin'
    if not (cmake / 'cmake').is_file() or not args.packager.is_file():
        p.error('Existing DevEco CMake and cargo-makepad are required')
    subprocess.run(['python3', str(ROOT / 'scripts/setup-home.py'), '--check'], cwd=ROOT, check=True)
    env = dict(os.environ, OCTOSENSE_WORKSPACE=str(ROOT / '.sources'))
    java_home = args.deveco_home / 'jbr/Contents/Home'
    if not (java_home / 'bin/java').is_file():
        p.error('The existing DevEco Java runtime is required')
    env['JAVA_HOME'] = str(java_home)
    env['PATH'] = os.pathsep.join([str(java_home / 'bin'), str(cmake), env.get('PATH', '')])
    env.pop('CARGO_TARGET_DIR', None)
    env.pop('MAKEPAD_REMOTE', None)
    if args.remote_port:
        env['MAKEPAD_REMOTE'] = str(args.remote_port)
    cargo_args = ['-p', 'octosense', '--release', '--locked']
    if args.offline:
        cargo_args.append('--offline')
    command = [str(args.packager), 'makepad', 'ohos', '--deveco-home=' + str(args.deveco_home), '--arch=aarch64']
    home = ROOT / 'home'
    subprocess.run(command + ['deveco'] + cargo_args, cwd=home, env=env, check=True)
    project = home / 'target/makepad-open-harmony/octosense'
    # The packager override can come from another checkout. Always take the
    # ArkTS shell and metadata from this product's pinned framework source.
    shutil.copytree(ROOT / '.sources/makepad/tools/open_harmony/deveco', project, dirs_exist_ok=True)
    app_path = project / 'AppScope/app.json5'
    app = read_json5(app_path)
    app['app']['bundleName'] = args.bundle_id
    app_path.write_text(json.dumps(app, indent=2) + '\n')
    for path in [project / 'AppScope/resources/base/element/string.json',
                 *project.glob('entry/src/main/resources/*/element/string.json')]:
        value = read_json5(path)
        for item in value['string']:
            if item['name'] in ('app_name', 'EntryAbility_label'):
                item['value'] = 'OctoSense Home'
        if path == project / 'entry/src/main/resources/base/element/string.json':
            reasons = {'camera_reason': 'Use the camera when you open Camera.',
                       'microphone_reason': 'Record sound when you record a video.',
                       'gallery_reason': 'Save photos and videos to your library.'}
            existing = {item['name'] for item in value['string']}
            value['string'].extend({'name': key, 'value': text} for key, text in reasons.items() if key not in existing)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    build_path = project / 'build-profile.json5'
    build = read_json5(build_path)
    build['app']['signingConfigs'] = configs
    build['app']['products'][0]['compatibleSdkVersion'] = args.compatible_sdk
    build_path.write_text(json.dumps(build, indent=2) + '\n')
    build_path.chmod(0o600)
    try:
        subprocess.run(command + ['build'] + cargo_args, cwd=home, env=env, check=True)
    finally:
        # Do not leave signing passwords in the generated workspace.
        build['app']['signingConfigs'] = []
        build_path.write_text(json.dumps(build, indent=2) + '\n')
    hap = project / 'entry/build/default/outputs/default/makepad-default-signed.hap'
    output = ROOT / 'out/home' / ('ohos-validation' if args.remote_port else 'ohos')
    output.mkdir(parents=True, exist_ok=True)
    shutil.copy2(hap, output / 'OctoSenseHome.hap')
    receipt = {
        'schema_version': 1, 'variant': 'ohos', 'bundle_id': args.bundle_id,
        'validation_remote': bool(args.remote_port),
        'source_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        'packager': str(args.packager),
        'profile_sha256': hashlib.sha256(profile).hexdigest(),
        'hap_sha256': hashlib.sha256(hap.read_bytes()).hexdigest(),
        'runtime': json.loads((home / 'native-runtime.lock.json').read_text()),
        'runtime_patches': json.loads((home / 'runtime-patches.lock.json').read_text()),
    }
    (output / 'build.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print('Built Home HAP:', output / 'OctoSenseHome.hap')


if __name__ == '__main__':
    main()
