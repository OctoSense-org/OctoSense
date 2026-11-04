#!/usr/bin/env python3
"""Native bundle-font regression (macOS graphical session, hidden window).

Build: cargo build --release -p octosense-card-host
Run:   python3 crates/card-host/tests/native_fonts.py
The disposable unsigned fixture requests no device/network capabilities. The
printed directory retains the real font-load log, native snapshot and PNG.
"""
import argparse, base64, json, os, re, shutil, socket, subprocess, tempfile, time, urllib.request
from pathlib import Path
repo = Path(__file__).resolve().parents[3]
binary = repo / 'target/release' / ('card-host.exe' if os.name == 'nt' else 'card-host')
assert binary.is_file(), 'Build card-host first (see this file docstring)'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--font', choices=['bundled', 'cjk-regular', 'cjk-bold'], default='bundled')
args = parser.parse_args()
font_source = {
    'bundled': 'assets/Body.ttf',
    'cjk-regular': 'makepad_widgets:resources/LXGWWenKaiRegular.ttf',
    'cjk-bold': 'makepad_widgets:resources/LXGWWenKaiBold.ttf',
}[args.font]
root = Path(tempfile.mkdtemp(prefix='octosense-font-card-'))
bundle = root / 'bundle'
shutil.copytree(repo / 'crates/app-hub/tests/fixtures/card', bundle)
(bundle / 'assets').mkdir()
shutil.copyfile(repo.parent / 'makepad/widgets/resources/Roboto-Regular.ttf', bundle / 'assets/Body.ttf')
card = (bundle / 'page.card').read_text(encoding='utf-8').replace('Focus Timer', '邮件与日历').replace('A quiet place to focus', 'Bundled font · 中文字体').replace('Your next focus session', '家庭活动 · Appointment').replace('No device permissions requested', '无需系统字体')
(bundle / 'page.card').write_text(card, encoding='utf-8')
p = bundle / 'kit/native/light/kit.json'
pack = json.loads(p.read_text(encoding='utf-8'))
for spec in pack['components'].values():
    if 'font_src' in spec['style']:
        spec['style']['font_src'] = font_source
p.write_text(json.dumps(pack, indent=2) + '\n', encoding='utf-8')
(bundle / 'icon.svg').write_text('<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#146"/></svg>')
(bundle / 'screen.png').write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+ip1sAAAAASUVORK5CYII='))
(bundle / 'manifest.json').write_text(json.dumps({'schema': 1, 'id': 'org.example.fontfixture', 'version': '1.0.0', 'name': 'Font fixture', 'integrity': {'bundle_blake3': ''}}))
(bundle / 'listing.json').write_text(json.dumps({'schema': 1, 'description': 'Synthetic font regression fixture', 'category': 'utilities', 'screenshots': ['screen.png'], 'icon': 'icon.svg', 'platforms': ['macos'], 'publisher': {'name': 'Fixture', 'support': 'https://example.test/support', 'privacy_policy_url': 'https://example.test/privacy'}, 'age_rating': 'all'}))
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
env = os.environ.copy()
env.update(MAKEPAD_HIDE_WINDOWS='1', MAKEPAD_SYSTEM_FONTS='0', MAKEPAD_TRACE_FONT_LOAD='1', MAKEPAD_REMOTE=str(port))
env.pop('MAKEPAD_FORCE_FOCUS', None)
log = (root / 'host.log').open('w', encoding='utf-8')
process = subprocess.Popen([str(binary), '--bundle', str(bundle), '--app-data', str(root / 'data'), '--allow-unsigned', '--stamp'], cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
base = f'http://127.0.0.1:{port}/'
font_load = re.compile('font load: member=latin path=http://127\\.0\\.0\\.1:\\d+/assets/Body\\.ttf .* bytes=' + str((bundle / 'assets/Body.ttf').stat().st_size) + '\\b')
if args.font != 'bundled':
    resource = (repo.parent / 'makepad/widgets/resources' / font_source.rsplit('/', 1)[1]).resolve()
    font_load = re.compile(r'font load: member=latin path=' + re.escape(str(resource)) + r' .* bytes=' + str(resource.stat().st_size) + r'\b')

def get(route):
    with urllib.request.urlopen(base + route, timeout=5) as response:
        return json.load(response)
try:
    end = time.monotonic() + 50
    while time.monotonic() < end:
        if process.poll() is not None:
            raise RuntimeError('card-host exited; inspect owned host.log')
        try:
            get('snap')
            if font_load.search((root / 'host.log').read_text(encoding='utf-8')):
                break
        except (OSError, ValueError):
            pass
        time.sleep(0.2)
    time.sleep(2)
    assert font_load.search((root / 'host.log').read_text(encoding='utf-8')), f'{args.font} font was not loaded; inspect {root}/host.log'
    snapshot = get('snap')
    (root / 'snapshot.json').write_text(json.dumps(snapshot, indent=2))
    capture = get('g?scale=1')
    shutil.copyfile(capture['png'], root / 'font-card.png')
    assert 'admitted — capabilities {}, hosts {}' in (root / 'host.log').read_text(encoding='utf-8'), 'Fixture must run without network grants'
    print(json.dumps({'root': str(root), 'font': args.font, 'screenshot': str(root / 'font-card.png'), 'snapshot_saved': True}))
finally:
    try:
        get('quit')
    except (OSError, ValueError):
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=5)
    log.close()
