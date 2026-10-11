#!/usr/bin/env python3
"""Installed GitHub Notes sign-in journey against a signed-out synthetic GitHub.

The host's real sign-in service, sheet and connection store run with
`--provider-fixture=github-sign-in`: a synthetic device-code endpoint, a token
endpoint whose answer the fixture file decides, and a fictional identity.
Nothing reaches github.com, so this cannot establish live OAuth.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
from notes_native import NotesNative as Native

APP = 'org.octosense.samples.githubnotes'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--host', type=Path, default=Path('target/release/examples/connected-app-host'))
    parser.add_argument('--installer', type=Path, default=Path('target/release/examples/connected-install'))
    parser.add_argument('--out', type=Path, default=Path('target/connected-notes-signin'))
    args = parser.parse_args()
    args.host, args.installer, args.bundle = args.host.resolve(), args.installer.resolve(), args.bundle.resolve()
    evidence = args.out / ('run-' + str(time.time_ns()))
    evidence.mkdir(parents=True)
    workspace = Path(__file__).resolve().parents[2]
    sources = [Path(__file__).resolve(), Path(__file__).with_name('native.py'), Path(__file__).with_name('notes_native.py'),
               workspace / 'crates/shell/examples/connected-app-host.rs',
               workspace / 'crates/oauth-service/src/acceptance_github.rs',
               workspace / 'crates/oauth-service/src/host.rs',
               workspace / 'crates/oauth-service/src/sign_in_code.rs',
               args.bundle / 'main.splash', args.bundle / 'manifest.json']
    hashes = {str(p.relative_to(workspace.parent)): digest(p) for p in sources}
    receipt = {'result': 'running', 'scope': 'signed installed UI and actual host sign-in service with synthetic GitHub I/O',
               'live_provider': False, 'binary_sha256': digest(args.host), 'installer_sha256': digest(args.installer),
               'source_sha256': hashes, 'checks': [], 'visual_review': 'pending'}
    ui = None
    with tempfile.TemporaryDirectory(prefix='connected-notes-signin-') as directory:
        profile = Path(directory) / 'apps'
        decision = profile / '.host/fixtures/github-sign-in.json'

        def decide(answer):
            decision.parent.mkdir(parents=True, exist_ok=True)
            decision.write_text(json.dumps({'decision': answer}))

        def sheet_closed():
            # The app updates as soon as the sign-in settles; the modal sheet
            # retires on its next status poll and blocks input until then.
            ui.wait(lambda: ui.find(identifier='oauth_subtitle', kind='Label') is None, timeout=20)

        def connect():
            # The card's primary button, then the sheet's own.
            ui.click('Connect GitHub')
            ui.label('wants to use your GitHub account', identifier='oauth_subtitle')
            ui.label('Read and update your public repositories')

        try:
            installed = subprocess.run([str(args.installer), '--keep-profile=' + str(profile), str(args.bundle)],
                                       text=True, capture_output=True, check=True)
            assert json.loads(installed.stdout)['apps'][0]['signed_install']
            ui = Native(args.host, ['--installed-app=' + APP, '--app-data=' + str(profile),
                                    '--provider-fixture=github-sign-in'], evidence, '01-sign-in')
            ui.label('Saved locally')
            assert 'CONNECTED_PROVIDER_FIXTURE' in ui.log_path.read_text()
            ui.click('Repository')
            ui.label('What GitHub Notes can reach')
            assert ui.find(text='Disconnect') is None, 'Disconnect offered with no account'
            ui.capture('01-connect-card')
            decide('pending')
            connect()
            ui.capture('02-consent')
            ui.click('Continue to GitHub')
            ui.label('WDJB-MJHT')
            ui.label('Code expires in')
            ui.click('Copy code')
            ui.wait(lambda: ui.find(text='Copied'))
            ui.capture('03-code')
            receipt['checks'].append('Sheet names the app, lists access, then shows the code with Copy, Open GitHub and the time left')
            decide('approve')
            sheet_closed()
            ui.wait(lambda: ui.find(text='fixture-writer', kind='Label'))
            ui.label('Connected · Public repositories only')
            ui.wait(lambda: ui.find(text='fixture-author/notes'))
            ui.capture('04-connected')
            receipt['checks'].append('Approval closes the sheet; the card names the account and access, and repositories load')
            ui.click('Disconnect')
            ui.label('Your note and its saved destination stay on this device')
            ui.click('Keep connected')
            ui.wait(lambda: ui.find(text='Use another account'))
            ui.click('Disconnect')
            ui.label('Your note and its saved destination stay on this device')
            ui.click('Disconnect')
            ui.label('Disconnected. Your note and its saved destination stay on this device.')
            ui.capture('05-disconnected')
            receipt['checks'].append('Disconnect asks first; Keep connected keeps it; confirming disconnects and says the note stays')
            decide('pending')
            connect()
            ui.click('Continue to GitHub')
            ui.label('WDJB-MJHT')
            decide('deny')
            ui.label('You declined on GitHub', identifier='failure')
            ui.capture('06-sheet-declined')
            ui.click('Close')
            sheet_closed()
            ui.label('Not connected')
            ui.label('You declined on GitHub, so nothing was connected.')
            ui.wait(lambda: ui.find(text='Try again'))
            ui.capture('07-app-declined')
            receipt['checks'].append('A decline reads as plain words in the sheet and the card, which offers Try again')
            decide('pending')
            ui.click('Try again')
            ui.label('wants to use your GitHub account', identifier='oauth_subtitle')
            ui.click('Cancel')
            sheet_closed()
            ui.label('Sign-in cancelled. Nothing was connected.')
            ui.wait(lambda: ui.find(text='Connect GitHub'))
            ui.capture('08-cancelled')
            receipt['checks'].append('Cancelling in the sheet leaves a neutral note and Connect GitHub')
            ui.check_logs()
            assert all(digest(p) == hashes[str(p.relative_to(workspace.parent))] for p in sources), 'Sources changed during acceptance'
            receipt['result'] = 'pass'
        except BaseException as error:
            receipt['result'] = 'fail'; receipt['failure'] = str(error)
            if ui:
                try: ui.capture('failure')
                except Exception: pass
            raise
        finally:
            if ui:
                receipt['actions'] = ui.actions
                ui.close()
            receipt['captures'] = {p.name: digest(p) for p in evidence.glob('*.png')}
            (evidence / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
            print('Evidence:', evidence)
    print('PASS: installed Notes sign-in with synthetic GitHub; native pixel review pending')


if __name__ == '__main__':
    run()
