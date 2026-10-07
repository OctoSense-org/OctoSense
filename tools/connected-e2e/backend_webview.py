#!/usr/bin/env python3
"""Native embedded-backend acceptance; fictional credentials and real WebView forms.

Uses only the host example's explicit fixture-only private command file for DOM
input and WK snapshot inspection. Never injects an auth callback, code, cookie,
account or token. The ordinary browser acceptance driver remains separate.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
from backend_login import APP, OTHER, PASSWORD, ROOT, BrowserNative, bundle, sha, wait

USER = 'fictional-reader@example.invalid'


class WebView:
    def __init__(self, native, control, private):
        self.native, self.control, self.private = native, control, private
        self.sequence = 0

    def request(self, operation, script=None, screenshot=None):
        self.sequence += 1
        result = self.private / f'webview-result-{self.sequence}.json'
        command = {'id': self.sequence, 'op': operation, 'result_path': str(result)}
        if script is not None:
            command['script'] = script
        if screenshot is not None:
            command['snapshot_path'] = str(screenshot)
        temporary = self.control.with_suffix('.next')
        temporary.write_text(json.dumps(command))
        temporary.replace(self.control)
        wait(lambda: result.is_file(), timeout=15)
        value = json.loads(result.read_text())
        if value.get('error') or value.get('ok') is False:
            raise AssertionError('Native fixture WebView operation failed')
        if screenshot:
            wait(lambda: screenshot.is_file(), timeout=15)
        return value

    def inspect(self, title=None, text=None):
        last = None
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            last = self.request('inspect')
            if last.get('readyState') == 'complete' and (title is None or last.get('title') == title) and (text is None or text in last.get('text', '')):
                return last
            time.sleep(.1)
        raise AssertionError('WebView page did not reach expected title/text')

    def evaluate(self, script):
        # Native completion reports success/failure only. No arbitrary page
        # return value, hidden fields or credential material is exported.
        self.request('eval', script=script)

    def click(self, selector):
        self.evaluate('(()=>{const e=document.querySelector(' + json.dumps(selector) + ');if(!e)throw new Error("Missing fixture control");e.scrollIntoView({block:"center"});e.click();})()')

    def credentials(self, password=PASSWORD):
        self.evaluate('(()=>{const u=document.querySelector("#username"),p=document.querySelector("#password");if(!u||!p)throw new Error("Missing fixture form");u.value=' + json.dumps(USER) + ';p.value=' + json.dumps(password) + ';u.dispatchEvent(new Event("input",{bubbles:true}));p.dispatchEvent(new Event("input",{bubbles:true}));p.scrollIntoView({block:"center"});})()')

    def capture(self, name, evidence):
        self.request('inspect', screenshot=evidence / (name + '-webview.png'))
        self.native.capture(name + '-host')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--installer', type=Path, required=True)
    parser.add_argument('--hub', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    run = args.out.resolve()
    run.mkdir(mode=0o700, parents=True, exist_ok=False)
    os.environ['RINX_DATA_DIR'] = str(run / 'rinx')
    evidence = run / 'evidence'
    evidence.mkdir()
    profile = run / 'apps'
    receipt = {'schema': 1, 'result': 'running', 'started_utc': datetime.now(timezone.utc).isoformat(),
               'case': 'Actual isolated native backend WebView form acceptance',
               'provider': 'Synthetic HTTP backend only; no real Google/GitHub accounts',
               'input': 'Makepad native host controls and feature-gated native DOM form input; no callback/cookie/token injection',
               'vault': 'Normal platform vault; no credential fixture override',
               'binary_sha256': sha(args.binary), 'installer_sha256': sha(args.installer),
               'runtime_base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
               'makepad_expected_tree': json.loads((ROOT / 'runtime-patches.lock.json').read_text())['makepad']['tree'],
               'makepad_base_commit': subprocess.check_output(['git', '-C', '.sources/makepad', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
               'checks': [], 'instrument_events': [], 'visual_review': 'pending'}
    sources = [Path(__file__).resolve(), ROOT / 'tools/connected-e2e/backend_login.py',
               ROOT / 'tools/connected-e2e/native.py', ROOT / 'tools/connected-e2e/backend-login/main.splash',
               ROOT / 'tools/backend-login-fixture.py', ROOT / 'crates/shell/examples/connected-app-host.rs',
               ROOT / 'crates/shell/examples/connected_support/mod.rs', ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
               ROOT / 'crates/oauth-service/Cargo.toml', ROOT / 'native-runtime.lock.json',
               ROOT / 'runtime-patches.lock.json']
    sources += sorted((ROOT / 'crates/oauth-service/src').rglob('*.rs'))
    sources += sorted((ROOT / 'tools/runtime-patches').glob('*.patch'))
    # Bind native implementation bytes even while a reviewed overlay is being
    # staged into the runtime patch stack by the repository maintainer.
    native_sources = ['platform/src/cx_api.rs', 'platform/src/event/event.rs',
        'platform/src/os/apple/apple_webview.rs', 'platform/src/os/apple/macos/macos.rs',
        'widgets/src/web_reader.rs']
    sources += [ROOT / '.sources/makepad' / name for name in native_sources]
    receipt['source_sha256'] = {str(p.relative_to(ROOT)): sha(p) for p in sources}
    log = (run / 'server.log').open('w')
    server = subprocess.Popen(['python3', str(ROOT / 'tools/backend-login-fixture.py'),
                               '--directory=' + str(run / 'server')], stdout=log, stderr=log)
    native = None
    sequence = 0
    try:
        metadata = wait(lambda: json.loads((run / 'server/metadata.json').read_text()) if (run / 'server/metadata.json').is_file() else None)
        for app in (APP, OTHER):
            registration = dict(metadata['registration'], app_id=app, id='fixture' if app == APP else 'fixture-other')
            (run / (app + '.json')).write_text(json.dumps(registration))
            bundle(run / app, app, args.hub)
        installed = subprocess.run([str(args.installer), '--keep-profile=' + str(profile),
                                    str(run / APP), str(run / OTHER)], check=True, capture_output=True, text=True)
        receipt['install_receipt'] = json.loads(installed.stdout)
        config = profile / '.host/oauth'
        config.mkdir(parents=True, exist_ok=True)
        (config / 'clients.json').write_text('{}\n')

        def events():
            path = run / 'server/events.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        def start(app):
            nonlocal native, sequence
            if native:
                receipt['instrument_events'] += native.actions
                native.check_logs()
                native.close()
            sequence += 1
            private = run / f'webview-{sequence}'
            private.mkdir()
            control = private / 'command.json'
            native = BrowserNative(args.binary.resolve(), ['--installed-app=' + app,
                '--app-data=' + str(profile), '--backend-fixture=' + str(run / (app + '.json')),
                '--webview-control=' + str(control)], evidence, f'native-{sequence}')
            native.label('', identifier='status')
            return WebView(native, control, private)

        def state(expected):
            native.wait(lambda: (native.find(identifier='status', kind='Label') or {}).get('t') == expected)

        def accounts():
            path = profile / '.host/oauth/connections.json'
            return json.loads(path.read_text()).get('entries', {}) if path.exists() else {}

        def open_view(view):
            offset = len(events())
            native.click('Connect in WebView')
            native.label('Continue to authorize', identifier='oauth_status')
            native.click('Continue')
            view.inspect(title='OctoSense synthetic backend')
            cookie = next(e for e in events()[offset:] if e['event'] == 'authorize_cookie')
            assert cookie['status'] == 204, 'A fresh auth WebView inherited another session cookie'

        def connected():
            state('Connected')
            native.wait(lambda: native.find(identifier='oauth_status', kind='Label') is None)

        view = start(APP)
        state('Disconnected')
        open_view(view)
        view.capture('01-native-form', evidence)
        before = len([e for e in events() if e['event'] == 'exchange'])
        native.click('Cancel')
        native.wait(lambda: native.find(identifier='oauth_status', kind='Label') is None)
        assert native.find(identifier='oauth_webview', kind=None) is None
        assert not accounts()
        assert len([e for e in events() if e['event'] == 'exchange']) == before
        native.capture('02-cancelled')
        receipt['checks'].append('Host Cancel dismisses real WebView; no token exchange or account is created')

        open_view(view)
        view.capture('02-pending-process-close', evidence)
        pending_process = native.child
        assert pending_process.poll() is None
        before = len([e for e in events() if e['event'] == 'exchange'])
        view = start(APP)  # Normal native /quit, then a new process; no force kill.
        assert pending_process.poll() == 0
        state('Disconnected')
        assert not accounts()
        assert len([e for e in events() if e['event'] == 'exchange']) == before
        native.capture('02-pending-process-reopened')
        receipt['checks'].append('Normal host process exit during pending WebView auth creates no account or exchange; a new process remains disconnected')

        open_view(view)
        view.click('#information')
        view.inspect(title='Fictional backend information')
        view.capture('03-information', evidence)
        native.click('Back')
        view.inspect(title='OctoSense synthetic backend')
        receipt['checks'].append('Native host Back returns from actual same-origin WebView navigation to login form')

        # A real same-origin navigation encounters an actual transport error.
        # Retry must recreate the isolated WebView while retaining the host's
        # pending PKCE request; no callback or auth result is supplied here.
        def availability(available):
            request = urllib.request.Request(metadata['origin'] + '/fixture/navigation-availability',
                data=json.dumps({'available': available}).encode(),
                headers={'Content-Type': 'application/json'}, method='POST')
            with urllib.request.urlopen(request, timeout=10) as response:
                assert response.status == 200
        availability(False)
        view.click('#connection_check')
        native.label('Could not load sign-in. Check your connection and retry.', identifier='oauth_status')
        native.capture('04-navigation-error-host')
        availability(True)
        offset = len(events())
        native.click('Retry')
        view.inspect(title='OctoSense synthetic backend')
        cookie = next(e for e in events()[offset:] if e['event'] == 'authorize_cookie')
        assert cookie['status'] == 204, 'Retry reused a prior auth WebView cookie jar'
        view.capture('04-retried', evidence)
        receipt['checks'].append('Real transport failure shows host error; Retry creates a clean WebView and returns to the form')

        view.credentials()
        view.capture('04-registration', evidence)
        view.click('#register')
        view.inspect(title='OctoSense synthetic backend', text='Account created. Sign in to continue.')
        view.credentials('Incorrect-fictional-password')
        view.click('#login')
        view.inspect(title='OctoSense synthetic backend', text='Username or password incorrect.')
        view.capture('05-invalid-password', evidence)
        view.credentials()
        view.click('#login')
        connected()
        native.click('Protected identity')
        state('Protected identity loaded')
        assert native.find(identifier='identity', kind='Label')['t'] == USER
        native.capture('06-connected')
        first = next(v['handle'] for v in accounts().values() if v['app_id'] == APP)
        receipt['checks'].append('Actual native WebView registration, credential error, login, intercepted fixed callback, PKCE exchange and protected identity')

        view = start(OTHER)
        state('Disconnected')
        (profile / OTHER / 'probe.json').write_text(json.dumps({'connection': first}))
        native.click('Probe other app')
        native.label('Other app denied:', identifier='status')
        open_view(view)
        view.credentials()
        view.capture('07-other-app-form', evidence)
        view.click('#login')
        connected()
        native.click('Protected identity')
        state('Protected identity loaded')
        assert native.find(identifier='identity', kind='Label')['t'] == USER
        assert next(v['handle'] for v in accounts().values() if v['app_id'] == OTHER) != first
        native.click('Probe other app')
        native.label('Other app denied:', identifier='status')
        native.capture('08-cross-app-denied')
        receipt['checks'].append('New app WebView receives no prior HttpOnly browser cookie and cannot use the first app connection before or after own login')
        assert any(e['event'] == 'session_cookie' and e['status'] == 200 for e in events())
        receipt['checks'].append('Server observes its real HttpOnly cookie on form submissions; no cookie was injected or read by driver')

        native.click('Disconnect')
        state('Disconnected')
        view = start(APP)
        state('Connected')
        native.click('Protected identity')
        state('Protected identity loaded')
        native.capture('09-cold-restored')
        native.click('Disconnect')
        state('Disconnected')
        native.click('Protected identity')
        state('Backend selected account changed; choose the account again')
        native.capture('10-logout-denied')
        assert accounts() == {}
        receipt['checks'].append('Native restart retains first connection, and host logout revokes both local handles and denies protected reads')
        native.check_logs()
        markers = ('code_challenge=', 'code_verifier=', 'access_token', 'refresh_token', '/auth/callback?')
        receipt['native_auth_material_log_markers_absent'] = not any(
            any(marker in path.read_text() for marker in markers)
            for path in evidence.glob('native-*.log'))
        assert receipt['native_auth_material_log_markers_absent'], 'Native auth log privacy check failed'
        receipt['server_events'] = events()
        assert len([e for e in events() if e['event'] == 'logout' and e['status'] == 200]) == 2
        receipt['result'] = 'PASS'
    except Exception as error:
        receipt['result'] = 'FAIL'
        receipt['error_type'] = type(error).__name__
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
        metadata_path = profile / '.host/oauth/connections.json'
        cleanup_ok = True
        if metadata_path.exists():
            remaining = json.loads(metadata_path.read_text()).get('entries', {})
            for app in sorted({v['app_id'] for v in remaining.values()}):
                if app not in (APP, OTHER):
                    cleanup_ok = False
                    continue
                cleanup = None
                try:
                    cleanup = BrowserNative(args.binary.resolve(), ['--installed-app=' + app,
                        '--app-data=' + str(profile), '--backend-fixture=' + str(run / (app + '.json'))],
                        evidence, 'cleanup-' + app.rsplit('.', 1)[-1])
                    cleanup.wait(lambda: (cleanup.find(identifier='status', kind='Label') or {}).get('t') == 'Connected')
                    cleanup.click('Disconnect')
                    cleanup.wait(lambda: (cleanup.find(identifier='status', kind='Label') or {}).get('t') == 'Disconnected', timeout=40)
                except Exception:
                    cleanup_ok = False
                finally:
                    if cleanup:
                        cleanup.close()
            cleanup_ok = cleanup_ok and not json.loads(metadata_path.read_text()).get('entries', {})
        receipt['local_connections_revoked'] = cleanup_ok
        receipt['vault_cleanup'] = 'Deletion requested through host disconnect; no independent vault readback'
        server.terminate()
        server.wait(timeout=10)
        log.close()
        receipt['finished_utc'] = datetime.now(timezone.utc).isoformat()
        receipt['source_changes_during_run'] = [str(p.relative_to(ROOT)) for p in sources
            if not p.is_file() or sha(p) != receipt['source_sha256'][str(p.relative_to(ROOT))]]
        receipt['binary_unchanged_during_run'] = sha(args.binary) == receipt['binary_sha256']
        receipt['native_processes_stopped'] = True
        receipt['synthetic_server_stopped'] = True
        (run / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps({'result': receipt['result'], 'receipt': str(run / 'receipt.json')}))


if __name__ == '__main__':
    main()
