#!/usr/bin/env python3
"""Real native host + isolated Chrome + synthetic HTTP backend acceptance.

Run with a Python environment containing Playwright, and an existing Chrome.
No real provider sign-in is automated. All account creation occurs in the
browser, and all token exchanges, vault persistence and revocation in the host.
The private run directory is never an artifact to publish wholesale.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from native import Native

APP = 'org.octosense.samples.backend'
OTHER = 'org.octosense.samples.backendother'
USER = 'fictional-reader@example.test'
PASSWORD = 'Synthetic-acceptance-only-42!'
ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def wait(check, timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(.1)
    raise AssertionError('Acceptance state did not settle')


class BrowserNative(Native):
    """Default input acknowledgements plus exact state checks, never input replay."""
    def call(self, route, **query):
        query.pop('wait', None)
        for attempt in range(4):
            try:
                return super().call(route, **query)
            except (urllib.error.URLError, RuntimeError, TimeoutError) as error:
                # Frame arming is read-only. Do not repeat an injected click/key.
                if route not in ('snap', 'g', 's'):
                    raise
                self.actions.append({'read_only_retry': route, 'attempt': attempt + 1,
                                     'error_type': type(error).__name__})
                if attempt == 3:
                    raise
                time.sleep(.2)


def bundle(path, app, hub):
    path.mkdir()
    shutil.copyfile(ROOT / 'tools/connected-e2e/backend-login/main.splash', path / 'main.splash')
    manifest = {'schema': 1, 'id': app, 'name': 'Backend acceptance', 'version': '0.1.0',
                'capabilities': ['storage', 'auth'], 'storage': {'accounts': True, 'max_bytes': 65536},
                'integrity': {'bundle_blake3': ''}}
    (path / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    # Internal signed-admission fixture, never a public listing or UI evidence.
    (path / 'fixture.svg').write_text('<svg xmlns="http://www.w3.org/2000/svg" width="640" height="640"><rect width="640" height="640" fill="#f4f7fc"/><text x="32" y="100" font-size="24">Internal backend login fixture</text><text x="32" y="150" font-size="18">Placeholder asset, not a native screenshot.</text></svg>')
    listing = {'schema': 1, 'subtitle': 'Internal browser login acceptance',
               'description': 'Private test fixture; not a published app. Placeholder listing artwork is not UX evidence.',
               'category': 'productivity', 'screenshots': ['fixture.svg'], 'icon': 'fixture.svg',
               'platforms': ['macos'], 'publisher': {'name': 'Acceptance fixture',
               'support': 'https://example.test/support', 'privacy_policy_url': 'https://example.test/privacy'},
               'age_rating': 'all', 'license': 'Apache-2.0'}
    (path / 'listing.json').write_text(json.dumps(listing, indent=2))
    subprocess.run([str(hub), 'stamp', str(path)], check=True, capture_output=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--installer', type=Path, required=True)
    parser.add_argument('--hub', type=Path, required=True)
    parser.add_argument('--chrome', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True, help='NEW private run directory')
    args = parser.parse_args()
    from playwright.sync_api import sync_playwright
    os.umask(0o077)
    run = args.out.resolve()
    run.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.environ['RINX_DATA_DIR'] = str(run / 'rinx')
    pixels = run / 'evidence'
    pixels.mkdir()
    receipt = {'schema': 1, 'result': 'running', 'started_utc': datetime.now(timezone.utc).isoformat(),
               'case': 'Real native backend login with synthetic server and actual browser callbacks',
               'provider': 'synthetic HTTP loopback; not live GitHub or Google',
               'vault': 'normal platform credential vault; no preseeded accounts or tokens',
               'browser_handoff': 'host-only LinkLabel URL copied privately by test example; exact URL opened in fresh Chrome context; OS-default-browser click omitted',
               'installation': 'ephemeral fixture signing, actual Store install and prepared launch',
               'input': 'Makepad native instrument, default input acknowledgement, no input replay',
               'binary_sha256': sha(args.binary), 'installer_sha256': sha(args.installer),
               'runtime_base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
               'visual_review': 'pending', 'checks': [], 'instrument_events': []}
    source_files = [Path(__file__).resolve(), ROOT / 'tools/backend-login-fixture.py',
                    ROOT / 'tools/connected-e2e/native.py',
                    ROOT / 'tools/connected-e2e/backend-login/main.splash',
                    ROOT / 'crates/shell/examples/connected-app-host.rs',
                    ROOT / 'crates/shell/examples/connected_support/mod.rs', ROOT / 'Cargo.lock']
    source_files += sorted((ROOT / 'crates/oauth-service/src').rglob('*.rs'))
    receipt['source_sha256'] = {str(p.relative_to(ROOT)): sha(p) for p in source_files}
    server_log = (run / 'server.log').open('w')
    server = subprocess.Popen(['python3', str(ROOT / 'tools/backend-login-fixture.py'),
                               '--directory=' + str(run / 'server'), '--token-ttl-seconds=45'], stdout=server_log, stderr=server_log)
    native = None
    native_count = 0
    profile = run / 'apps'
    try:
        metadata = wait(lambda: json.loads((run / 'server/metadata.json').read_text())
                        if (run / 'server/metadata.json').exists() else None)
        registration = metadata['registration']
        for app in (APP, OTHER):
            spec = dict(registration, app_id=app, id='fixture' if app == APP else 'fixture-other')
            (run / (app + '.json')).write_text(json.dumps(spec))
            bundle(run / app, app, args.hub)
        installed = subprocess.run([str(args.installer), '--keep-profile=' + str(profile),
                                    str(run / APP), str(run / OTHER)], check=True, capture_output=True, text=True)
        receipt['install_receipt'] = json.loads(installed.stdout)
        oauth = profile / '.host/oauth'
        oauth.mkdir(parents=True, exist_ok=True)
        (oauth / 'clients.json').write_text('{}\n')

        def start(app):
            nonlocal native, native_count
            if native:
                receipt['instrument_events'] += native.actions
                native.check_logs()
                native.close()
            native_count += 1
            capture = run / ('browser-' + str(native_count) + '.private')
            native = BrowserNative(args.binary.resolve(), ['--installed-app=' + app,
                                   '--app-data=' + str(profile), '--backend-fixture=' + str(run / (app + '.json')),
                                   '--capture-browser-url=' + str(capture)], pixels, 'native-' + str(native_count))
            native.label('', identifier='status')
            return capture

        def state(expected):
            native.wait(lambda: (native.find(identifier='status', kind='Label') or {}).get('t') == expected)

        def me():
            native.click('Protected identity')
            state('Protected identity loaded')
            assert native.find(identifier='identity', kind='Label')['t'] == USER

        def records():
            path = profile / '.host/oauth/connections.json'
            return json.loads(path.read_text()) if path.exists() else {}

        def handle(app):
            entries = records()['entries']
            return next(value['handle'] for value in entries.values() if value['app_id'] == app)

        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(executable_path=str(args.chrome), headless=True)
            context = browser.new_context(viewport={'width': 1000, 'height': 800})
            page = context.new_page()
            receipt['browser_diagnostics'] = []
            def browser_diagnostic(message):
                if message.type != 'error':
                    return
                path = urllib.parse.urlsplit(message.location.get('url', '')).path
                receipt['browser_diagnostics'].append({
                    'type': message.type,
                    'mentions_csp': 'Content Security Policy' in message.text,
                    'mentions_form_action': 'form-action' in message.text,
                    'http_404': '404' in message.text,
                    'favicon_request': path == '/favicon.ico',
                })
            page.on('console', browser_diagnostic)

            def login(capture, register=False, label='login'):
                native.click('Connect backend')
                native.label('Continue to authorize', identifier='oauth_status')
                native.capture(label + '-consent')
                native.click('Continue')
                url = wait(lambda: capture.read_text() if capture.exists() else None)
                parsed = urllib.parse.urlsplit(url)
                assert parsed.scheme == 'http' and parsed.netloc == urllib.parse.urlsplit(metadata['origin']).netloc
                assert parsed.path == '/authorize'
                page.goto(url)
                page.locator('#username').fill(USER)
                page.locator('#password').fill(PASSWORD)
                if register:
                    page.screenshot(path=str(pixels / (label + '-browser-registration.png')))
                    page.locator('#register').click()
                    assert 'Account created' in page.locator('#notice').inner_text()
                    page.locator('#username').fill(USER)
                    page.locator('#password').fill(PASSWORD)
                page.screenshot(path=str(pixels / (label + '-browser-login.png')))
                page.locator('#login').click()
                try:
                    page.wait_for_url('http://127.0.0.1:**/oauth/callback**')
                except Exception:
                    page.screenshot(path=str(pixels / (label + '-browser-failure.png')))
                    raise
                state('Connected')
                # The original app callback settles before the host sheet's
                # next status poll retires the modal. Do not click through it.
                native.wait(lambda: native.find(identifier='oauth_status', kind='Label') is None)
                native.capture(label + '-connected')

            capture = start(APP)
            state('Disconnected')
            login(capture, register=True, label='01-register')
            me()
            native.capture('02-protected-data')
            fault = urllib.request.Request(metadata['origin'] + '/fixture/fail-next-me',
                data=b'{"count":1}', headers={'Content-Type': 'application/json'}, method='POST')
            with urllib.request.urlopen(fault, timeout=10) as response:
                assert response.status == 200
            native.click('Protected identity')
            state('Backend request rejected; sign in again if needed')
            native.capture('02-refresh-identity-failure')
            me()
            native.capture('02-refresh-recovered')
            receipt['checks'].append('Short-lived token forces refresh; transient protected-data failure followed by another refresh recovers using durably stored rotated credential')
            first_handle = handle(APP)
            receipt['checks'].append('Fresh browser registration, password sign-in, real PKCE callback and protected identity')

            start(APP)
            state('Connected')
            assert handle(APP) == first_handle
            me()
            native.capture('03-cold-restored')
            receipt['checks'].append('Native process restart restores same app-bound connection from platform vault and reloads protected data')
            native.click('Disconnect')
            state('Disconnected')
            native.click('Protected identity')
            state('Backend selected account changed; choose the account again')
            native.capture('04-logout-denied')
            assert not any(v['app_id'] == APP for v in records()['entries'].values())
            receipt['checks'].append('Logout durably removes connection and protected identity is denied')

            capture = start(APP)
            state('Disconnected')
            login(capture, label='05-repeat-login')
            me()
            repeated_handle = handle(APP)
            assert repeated_handle != first_handle
            receipt['checks'].append('Existing fictional user signs in again through browser without registration; fresh connection issued')

            capture = start(OTHER)
            state('Disconnected')
            (profile / OTHER / 'probe.json').write_text(json.dumps({'connection': repeated_handle}))
            native.click('Probe other app')
            native.label('Other app denied:', identifier='status')
            native.capture('06-other-app-denied')
            login(capture, label='07-other-app-login')
            me()
            assert handle(OTHER) != repeated_handle
            native.click('Probe other app')
            native.label('Other app denied:', identifier='status')
            native.capture('08-other-app-own-login-isolation')
            receipt['checks'].append('Second signed app cannot access first handle before or after its own genuine browser login')
            native.click('Disconnect')
            state('Disconnected')
            start(APP)
            state('Connected')
            native.click('Disconnect')
            state('Disconnected')
            assert records()['entries'] == {}
            receipt['checks'].append('Both fictional handles durably revoked through real host service; platform-vault deletion requested (deletion not independently read back)')
            browser.close()
        events = [json.loads(line) for line in (run / 'server/events.jsonl').read_text().splitlines()]
        receipt['server_events'] = events
        for expected in ('register', 'login', 'exchange', 'me', 'logout'):
            assert any(event['event'] == expected and 200 <= event['status'] < 400 for event in events), expected
        receipt['result'] = 'PASS'
    except Exception as error:
        receipt['result'] = 'FAIL'
        receipt['error_type'] = type(error).__name__
        # Full exceptions may contain callback URLs. Keep raw failure only privately.
        (run / 'private-error.txt').write_text(str(error))
        if native:
            try:
                native.capture('FAILURE')
            except Exception:
                pass
        raise
    finally:
        if native:
            receipt['instrument_events'] += native.actions
            native.close()
        # Failed runs must not strand fictional tokens in the user's OS vault.
        # Reopen only our two signed fixture apps and use normal host disconnect.
        metadata_path = profile / '.host/oauth/connections.json'
        cleanup_ok = True
        if metadata_path.exists():
            remaining = json.loads(metadata_path.read_text()).get('entries', {})
            for app in sorted({entry['app_id'] for entry in remaining.values()}):
                if app not in (APP, OTHER):
                    cleanup_ok = False
                    continue
                cleanup = None
                try:
                    cleanup = BrowserNative(args.binary.resolve(), ['--installed-app=' + app,
                        '--app-data=' + str(profile), '--backend-fixture=' + str(run / (app + '.json'))],
                        pixels, 'cleanup-' + app.rsplit('.', 1)[-1])
                    cleanup.wait(lambda: (cleanup.find(identifier='status', kind='Label') or {}).get('t') == 'Connected')
                    cleanup.click('Disconnect')
                    cleanup.wait(lambda: (cleanup.find(identifier='status', kind='Label') or {}).get('t') == 'Disconnected', timeout=40)
                except Exception:
                    cleanup_ok = False
                finally:
                    if cleanup:
                        receipt['instrument_events'] += cleanup.actions
                        cleanup.close()
            cleanup_ok = cleanup_ok and not json.loads(metadata_path.read_text()).get('entries', {})
        receipt['local_connections_revoked'] = cleanup_ok
        receipt['vault_cleanup'] = 'Deletion requested through host disconnect; no independent vault readback'
        server.terminate()
        server.wait(timeout=10)
        server_log.close()
        receipt['finished_utc'] = datetime.now(timezone.utc).isoformat()
        receipt['native_processes_stopped'] = True
        receipt['synthetic_server_stopped'] = True
        (run / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps({'result': receipt['result'], 'receipt': str(run / 'receipt.json')}))


if __name__ == '__main__':
    main()
