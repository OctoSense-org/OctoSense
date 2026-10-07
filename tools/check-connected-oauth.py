#!/usr/bin/env python3
"""Native, credential-free sign-in sheet regression. Never opens provider login.

Usage: python3 tools/check-connected-oauth.py --bundle <GitHub Notes bundle>
The bundle must be stamped. Uses connected-app-host's real ordinary admission
and services in a fresh temporary profile, not a synthetic OAuth service.
Every run keeps its own receipt, logs, native pixels and visible widget snapshots,
including failures. Provider credentials and profile files are never copied out.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import struct
import zlib
import tempfile
import time
import urllib.parse
import urllib.request


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_rgb(path):
    """Read Makepad's non-interlaced RGB/RGBA8 PNGs without external packages."""
    data = path.read_bytes()
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('Not a PNG')
    position, packed = 8, bytearray()
    while position < len(data):
        length = struct.unpack('>I', data[position:position + 4])[0]
        kind = data[position + 4:position + 8]
        chunk = data[position + 8:position + 8 + length]
        if kind == b'IHDR':
            width, height, depth, color, compression, filtering, interlace = struct.unpack('>IIBBBBB', chunk)
            if depth != 8 or color not in (2, 6) or compression or filtering or interlace:
                raise ValueError('Expected native RGB/RGBA8 non-interlaced PNG')
        elif kind == b'IDAT':
            packed.extend(chunk)
        position += length + 12
    channels = 4 if color == 6 else 3
    stride = width * channels
    raw = zlib.decompress(packed)
    if len(raw) != height * (stride + 1):
        raise ValueError('Wrong PNG data length')
    previous = bytearray(stride)
    output = bytearray()
    for y in range(height):
        offset = y * (stride + 1)
        mode = raw[offset]
        row = bytearray(raw[offset + 1:offset + stride + 1])
        for x in range(stride):
            left = row[x - channels] if x >= channels else 0
            above = previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if mode == 1:
                predictor = left
            elif mode == 2:
                predictor = above
            elif mode == 3:
                predictor = (left + above) // 2
            elif mode == 4:
                prediction = left + above - corner
                distances = [abs(prediction - value) for value in (left, above, corner)]
                predictor = (left, above, corner)[distances.index(min(distances))]
            elif mode == 0:
                predictor = 0
            else:
                raise ValueError('Unknown PNG filter')
            row[x] = (row[x] + predictor) & 255
        for x in range(0, stride, channels):
            output.extend(row[x:x + 3])
        previous = row
    return width, height, output


def dark_pixels(image, rect, logical_size):
    width, height, rgb = image
    sx, sy = width / logical_size[0], height / logical_size[1]
    x, y, w, h = rect
    count = 0
    for py in range(max(0, int(y * sy)), min(height, int((y + h) * sy))):
        for px in range(max(0, int(x * sx)), min(width, int((x + w) * sx))):
            offset = (py * width + px) * 3
            if max(rgb[offset:offset + 3]) < 180:
                count += 1
    return count


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bundle', required=True, type=Path)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/connected-app-host'))
    parser.add_argument('--out', type=Path, default=Path('target/connected-oauth-native'))
    args = parser.parse_args()
    args.bundle = args.bundle.resolve()
    args.binary = args.binary.resolve()
    workspace = Path(__file__).resolve().parent.parent
    run = args.out / (datetime.now(timezone.utc).strftime('run-%Y%m%dT%H%M%SZ-') + str(time.time_ns()))
    run.mkdir(parents=True)
    sources = [Path(__file__).resolve(), workspace / 'crates/shell/examples/connected-app-host.rs',
               workspace / 'crates/markdown-editor/src/lib.rs', workspace / 'runtime-patches.lock.json',
               workspace / 'Cargo.lock']
    sources += sorted((workspace / 'crates/oauth-service/src').rglob('*.rs'))
    sources += sorted(args.bundle.rglob('*.splash'))
    sources += [args.bundle / 'manifest.json', args.bundle / 'tools.json']
    source_hashes = {str(path): sha256(path) for path in sources if path.is_file()}
    result = {'case': 'Native OAuth consent, missing host registration, cancellation and draft retention',
              'result': 'running', 'platform': 'macOS hidden native window',
              'external_sign_in': 'not attempted; no provider registration or account',
              'binary_sha256': sha256(args.binary), 'source_sha256': source_hashes,
              'bundle_manifest_sha256': sha256(args.bundle / 'manifest.json'),
              'driver': 'Codex via native Makepad instrument; no physical approval', 'steps': [],
              'visual_review': 'pending; functional assertions do not establish pixel acceptance'}
    endpoint = None
    with tempfile.TemporaryDirectory(prefix='connected-oauth-check-') as directory:
        root = Path(directory)
        shutil.copytree(args.bundle, root / 'bundle')
        # Complete operator override disables release registrations for this
        # credential-free regression, even in a distributor-configured build.
        registration_path = root / 'profile/.host/oauth/clients.json'
        registration_path.parent.mkdir(parents=True)
        registration_path.write_text('{}\n')
        with (root / 'native.log').open('w+') as log:
            child = subprocess.Popen([str(args.binary), '--bundle=' + str(root / 'bundle'),
                                      '--app-data=' + str(root / 'profile'), '--remote'],
                                     env={**os.environ, 'MAKEPAD_HIDE_WINDOWS': '1'}, stdout=log, stderr=log)
            result['owned_pid'] = child.pid

            def call(path, **query):
                suffix = '?' + urllib.parse.urlencode(query) if query else ''
                with urllib.request.urlopen(endpoint + path + suffix, timeout=10) as response:
                    data = json.load(response)
                if 'err' in data:
                    raise RuntimeError(data['err'])
                return data

            def rows():
                # Default /snap excludes hidden widgets. all=1 would allow an
                # invisible underlying app button to match a host sheet action.
                return [row for row in call('/snap')['s']
                        if row.get('ty') != 'Splash' and row.get('r', [0, 0, 0, 0])[2] > 0
                        and row['r'][3] > 0]

            def wait_for(text, button=False, widget_id=None):
                until = time.monotonic() + 12
                while time.monotonic() < until:
                    visible = rows()
                    if button:
                        matches = [row for row in visible if row.get('ty') == 'Button'
                                   and row.get('t') == text]
                    else:
                        matches = [row for row in visible if row.get('ty') in ('Label', 'LinkLabel')
                                   and text in row.get('t', '')]
                    if widget_id is not None:
                        matches = [row for row in matches if row.get('i') == widget_id]
                    if len(matches) == 1:
                        return matches[0]
                    if button and len(matches) > 1:
                        raise AssertionError('Ambiguous visible button: ' + text)
                    time.sleep(.1)
                raise AssertionError('Missing visible ' + ('button: ' if button else 'state: ') + text
                                     + (' in ' + widget_id if widget_id else ''))

            def click(text):
                row = wait_for(text, button=True)
                x, y, width, height = row['r']
                call('/click', x=x + width / 2, y=y + height / 2, wait=1)
                result['steps'].append('click ' + text)

            def wait_widget(widget_id, kind):
                until = time.monotonic() + 12
                while time.monotonic() < until:
                    matches = [row for row in rows() if row.get('i') == widget_id
                               and row.get('ty') == kind]
                    if len(matches) == 1:
                        return matches[0]
                    if len(matches) > 1:
                        raise AssertionError('Ambiguous visible widget: ' + widget_id)
                    time.sleep(.1)
                raise AssertionError('Missing visible widget: ' + widget_id)

            def click_widget(widget_id):
                row = wait_widget(widget_id, 'Button')
                x, y, width, height = row['r']
                call('/click', x=x + width / 2, y=y + height / 2, wait=1)
                result['steps'].append('click widget ' + widget_id)

            def capture(name):
                source = Path(call('/g')['png'])
                shutil.copyfile(source, run / (name + '.png'))
                result.setdefault('native_log_bytes_at_capture', {})[name] = (root / 'native.log').stat().st_size
                (run / (name + '.snapshot.json')).write_text(json.dumps(rows(), indent=2) + '\n')

            try:
                deadline = time.monotonic() + 25
                while time.monotonic() < deadline:
                    log.flush()
                    text = (root / 'native.log').read_text()
                    match = re.search(r'listening on (127\.0\.0\.1:\d+)', text)
                    if match:
                        endpoint = 'http://' + match[1]
                        break
                    if child.poll() is not None:
                        raise RuntimeError('Native host exited before starting instrument')
                    time.sleep(.1)
                if not endpoint:
                    raise RuntimeError('Native instrument did not start')
                # The native editor uses icon actions, not the older text tabs.
                wait_widget('markdown', 'TextInput')
                # The sample boots its saved draft on a 50 ms startup timer.
                # Visible native widgets alone do not mean that boot has run.
                time.sleep(.25)
                click_widget('source_mode')
                field = wait_widget('markdown', 'TextInput')
                x, y, width, height = field['r']
                call('/click', x=x + width / 2, y=y + min(height / 2, 20), wait=1)
                note = '# Consent regression\n\nKeep this draft: café, 谢谢。\n'
                call('/t', t=note, wait=1)
                assert next(row for row in rows() if row.get('i') == 'markdown')['t'] == note
                app_id = json.loads((args.bundle / 'manifest.json').read_text())['id']
                deadline = time.monotonic() + 5
                while True:
                    drafts = [json.loads(path.read_text()) for path in
                              (root / 'profile' / app_id).glob('draft-*.json')]
                    if drafts and max(drafts, key=lambda item: item['revision'])['draft']['content'] == note:
                        break
                    if time.monotonic() >= deadline:
                        raise AssertionError('Exact local draft did not persist before OAuth')
                    time.sleep(.1)
                capture('00-saved-draft')
                click_widget('repository_button')
                click('Connect public repositories')
                wait_for('Connect GitHub')
                wait_for('Read and update your public repositories')
                capture('01-host-consent')
                retained = [row for row in rows() if row.get('ty') == 'Label'
                            and (row.get('t') == 'Connect GitHub' or 'requests:' in row.get('t', ''))]
                logical_size = call('/s')['w'][0]['sz']
                click('Continue')
                # The snapshot contains geometrically visible app widgets below
                # the opaque host sheet. An arbitrary status-label match could
                # therefore accept an error shown only behind the sheet.
                wait_for('GitHub sign-in is unavailable in this build', widget_id='oauth_status')
                capture('02-missing-registration')
                before = png_rgb(run / '01-host-consent.png')
                after = png_rgb(run / '02-missing-registration.png')
                result['retained_ink'] = []
                for row in retained:
                    old = dark_pixels(before, row['r'], logical_size)
                    new = dark_pixels(after, row['r'], logical_size)
                    result['retained_ink'].append({'text': row['t'], 'before': old, 'after': new})
                assert len(retained) == 2, 'Expected the static title and consent descriptions'
                if any(row['before'] < 100 or row['after'] < row['before'] * .95 for row in result['retained_ink']):
                    result['visual_review'] = 'fail: static consent ink disappeared after the asynchronous status callback'
                    raise AssertionError(result['visual_review'])
                result['visual_review'] = 'static consent ink preserved; manual full-frame review still required'
                click('Cancel')
                wait_for('Back to note', button=True)
                click('Back to note')
                wait_widget('markdown', 'TextInput')
                assert next(row for row in rows() if row.get('i') == 'markdown')['t'] == note
                capture('03-return-to-draft')
                click_widget('repository_button')
                click('Connect public repositories')
                wait_for('Connect GitHub')
                click('Cancel')
                wait_for('Back to note', button=True)
                click('Back to note')
                assert next(row for row in rows() if row.get('i') == 'markdown')['t'] == note
                capture('04-cancel-before-continue')
                # Account selection uses the real host store/service, with
                # metadata-only fixtures: no scopes, tokens or provider calls.
                first = '00000000-0000-4000-8000-000000000001'
                second = '00000000-0000-4000-8000-000000000002'
                metadata = {'entries': {}, 'active': {app_id: first}}
                for handle, label in [(first, 'Fixture account A'), (second, 'Fixture account B')]:
                    metadata['entries'][handle] = {'handle': handle, 'app_id': app_id, 'provider': 'github',
                                                  'subject': label, 'label': label, 'scopes': [], 'expires_at': None}
                metadata_path = root / 'profile/.host/oauth/connections.json'
                metadata_path.parent.mkdir(parents=True, exist_ok=True)
                metadata_path.write_text(json.dumps(metadata))
                click_widget('repository_button')
                wait_for('Selected · Fixture account A', button=True)
                click('Fixture account B')
                wait_for('Selected · Fixture account B', button=True)
                wait_for('This operation needs additional authorization')
                assert json.loads(metadata_path.read_text())['active'][app_id] == second
                capture('05-selected-host-account')
                click('Back to note')
                assert next(row for row in rows() if row.get('i') == 'markdown')['t'] == note
                drafts = [json.loads(path.read_text()) for path in (root / 'profile' / app_id).glob('draft-*.json')]
                saved = max(drafts, key=lambda item: item['revision'])['draft']
                assert saved['connection'] == '' and saved['content'] == note, 'Account switch rebound the local draft'
                result['account_selection'] = 'pass: host active A loaded, B selected and persisted; unbound local draft unchanged'
                result['account_fixture'] = 'Synthetic metadata only, no scopes or credentials; provider reads stop at missing authorization'
                capture('06-account-selection-retains-draft')
                errors = [line for line in call('/log', n=100)['l']
                          if any(word in line for word in ('ScriptError', '[ERROR]', 'panicked'))]
                if errors:
                    raise AssertionError('Runtime errors: ' + '\n'.join(errors))
                changed = [path for path, digest in source_hashes.items() if sha256(Path(path)) != digest]
                if changed:
                    raise AssertionError('Sources changed during validation: ' + ', '.join(changed))
                result['result'] = 'pass'
                print('FUNCTIONAL PASS: real host sign-in sheet → missing registration → cancel → local draft; pixel review pending')
            except BaseException as error:
                result['result'] = 'fail'
                result['failure'] = str(error)
                if endpoint and child.poll() is None:
                    try:
                        capture('failure')
                    except Exception as capture_error:
                        result['capture_error'] = str(capture_error)
                raise
            finally:
                if endpoint and child.poll() is None:
                    try:
                        urllib.request.urlopen(endpoint + '/quit', timeout=5).close()
                    except OSError:
                        child.terminate()
                try:
                    child.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                log.flush()
                shutil.copyfile(root / 'native.log', run / 'native.log')
                result['exit_code'] = child.returncode
                result['captures'] = {path.name: sha256(path) for path in run.glob('*.png')}
                (run / 'receipt.json').write_text(json.dumps(result, indent=2) + '\n')
                (args.out / 'latest.json').write_text(json.dumps({'run': str(run.resolve()), 'result': result['result']}, indent=2) + '\n')
                print('Evidence:', run)


if __name__ == '__main__':
    main()
