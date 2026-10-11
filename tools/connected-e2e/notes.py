#!/usr/bin/env python3
"""Installed GitHub Notes native journey with a compile-only synthetic provider.

Never supplies real provider registrations, tokens or a real repository target.
This proves application/service flow against synthetic provider behavior; it
cannot establish live OAuth, GitHub delivery or physical approval.
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
    parser.add_argument('--out', type=Path, default=Path('target/connected-notes-e2e'))
    args = parser.parse_args()
    args.host, args.installer, args.bundle = args.host.resolve(), args.installer.resolve(), args.bundle.resolve()
    evidence = args.out / ('run-' + str(time.time_ns()))
    evidence.mkdir(parents=True)
    workspace = Path(__file__).resolve().parents[2]
    sources = [Path(__file__).resolve(), Path(__file__).with_name('native.py'), Path(__file__).with_name('notes_native.py'), workspace / 'Cargo.lock',
               workspace / 'crates/shell/examples/connected-app-host.rs',
               workspace / 'crates/shell/examples/connected_support/mod.rs',
               workspace / 'crates/oauth-service/src/acceptance_github.rs',
               workspace / 'crates/oauth-service/src/acceptance_fixtures.rs',
               workspace / 'crates/oauth-service/src/host_api.rs', workspace / 'crates/oauth-service/src/api.rs',
               workspace / 'crates/markdown-editor/src/lib.rs', args.bundle / 'main.splash', args.bundle / 'manifest.json']
    sources += sorted((workspace/'crates/markdown-editor').glob('src/*.rs'))
    sources += sorted((workspace/'crates/markdown-editor/resources').rglob('*.svg'))
    sources = list(dict.fromkeys(sources))
    hashes = {str(p.relative_to(workspace.parent)): digest(p) for p in sources}
    receipt = {'result': 'running', 'scope': 'signed installed UI and actual host services with synthetic GitHub I/O',
               'live_provider': False, 'physical_approval': False, 'driver': 'Codex native Makepad instrument',
               'binary_sha256': digest(args.host), 'installer_sha256': digest(args.installer),
               'source_sha256': hashes, 'checks': [], 'visual_review': 'pending'}
    ui = None
    sessions = []
    with tempfile.TemporaryDirectory(prefix='connected-notes-installed-') as directory:
        profile = Path(directory) / 'apps'
        provider = profile / '.host/fixtures/github-state.json'
        control = profile / '.host/fixtures/github-control.json'
        def start(name):
            instance = Native(args.host, ['--installed-app=' + APP, '--app-data=' + str(profile),
                                         '--provider-fixture=github'], evidence, name)
            sessions.append(instance)
            instance.label('Saved locally')
            assert 'CONNECTED_INSTALLED' in instance.log_path.read_text()
            assert 'CONNECTED_PROVIDER_FIXTURE' in instance.log_path.read_text()
            return instance
        def content():
            return ui.find(identifier='markdown', kind='TextInput')['t']
        def persisted():
            drafts = [json.loads(p.read_text()) for p in (profile / APP).glob('draft-*.json')]
            return max(drafts, key=lambda d:d['revision'])['draft'] if drafts else {}
        try:
            installed = subprocess.run([str(args.installer), '--keep-profile=' + str(profile), str(args.bundle)],
                                       text=True, capture_output=True, check=True)
            installation = json.loads(installed.stdout)
            assert installation['apps'][0]['signed_install']
            receipt['installation'] = installation
            ui = start('01-first-launch')
            ui.tab('Markdown')
            local = '# Unsent local note\n\nKeep café and 谢谢 while choosing a repository.\n'
            ui.field('markdown', local)
            ui.wait(lambda: persisted().get('content') == local)
            ui.tab('Preview')
            ui.capture('01-installed-preview')
            ui.tab('Markdown')
            assert content() == local
            ui.check_logs(); receipt['checks'].append('Signed Store launch, Unicode source edit and Preview round trip')
            ui.close(); ui = start('02-restart')
            ui.tab('Markdown')
            assert content() == local
            receipt['checks'].append('Verified installed restart restores exact unsent local note')
            ui.click('Repository')
            ui.wait(lambda: ui.find(text='Fixture GitHub · synthetic provider', kind='Label'))
            ui.click('Refresh')
            ui.label('Choose a repository')
            ui.click('Next page'); ui.label('Choose a repository · page 2')
            ui.label('No repositories on this page'); ui.click('Previous page')
            ui.label('Choose a repository · page 1')
            ui.click('fixture-author/empty'); ui.label('fixture-author/empty · main')
            ui.reachable(text='No Markdown files here · choose a new path below', kind='Label')
            ui.capture('02-empty-repository')
            receipt['checks'].append('Empty repository and empty paginated response keep navigation usable')
            ui.click('fixture-author/notes')
            ui.label('fixture-author/notes · main')
            ui.click('docs')
            ui.click('docs/second-note.md')
            ui.click('Keep current draft')
            ui.click('Back to note')
            assert content() == local
            receipt['checks'].append('Dirty note replacement refusal preserves the original note')
            ui.click('Repository'); ui.click('docs/second-note.md'); ui.click('Open & keep recovery')
            ui.label('Opened docs/second-note.md')
            ui.tab('Markdown')
            assert 'Original appointment: Tuesday at 09:00.' in content()
            edited = '# Delivery notes\n\nUpdated appointment: Wednesday at 14:30.\n\nBring café and 谢谢。\n\n| Task | State |\n| --- | --- |\n| Review | Ready |\n'
            ui.field('markdown', edited)
            ui.label('Saved locally')
            ui.capture('02-before-save')
            ui.click('Save')
            ui.label('Save Markdown to GitHub')
            ui.label('docs/second-note.md')
            ui.label(edited)
            ui.capture('02-exact-host-review')
            assert json.loads(provider.read_text())['revision'] == 0
            ui.click('Back to editing')
            ui.label('Save cancelled')
            assert content() == edited
            ui.capture('03-cancel-retains-edit')
            receipt['checks'].append('Actual host review shows exact edited Markdown and destination; cancel makes no provider write')
            ui.click('Save'); ui.label('Save Markdown to GitHub'); ui.click('Approve & Save')
            ui.label('GitHub committed')
            ui.wait(lambda: not ui.find(text='Save Markdown to GitHub', kind='Label'))
            remote = json.loads(provider.read_text())
            assert remote['files']['docs/second-note.md']['content'] == edited
            assert remote['last_commit']['path'] == 'docs/second-note.md'
            assert remote['revision'] == 1
            ui.wait(lambda: not persisted()['dirty'])
            ui.capture('04-synthetic-commit-confirmed')
            receipt['checks'].append('Reviewed save invokes real GitHub API adapter; synthetic provider content/commit SHA and local clean state agree')
            conflicting = edited + '\nA newer unsaved change must survive a conflict.\n'
            ui.field('markdown', conflicting)
            control.write_text(json.dumps({'next_save':'conflict'}))
            ui.click('Save'); ui.label('Save Markdown to GitHub'); ui.click('Approve & Save')
            ui.label('The remote content changed', identifier='connector_review_status')
            ui.capture('05-provider-conflict')
            assert json.loads(provider.read_text())['revision'] == 1
            ui.click('Back to editing')
            assert content() == conflicting
            ui.wait(lambda: persisted().get('content') == conflicting)
            assert not ui.find(text='Save Markdown to GitHub', kind='Label')
            receipt['checks'].append('409 conflict keeps the changed local note and never retries or overwrites')
            ui.click('Repository')
            ui.field('path_entry', 'notes/created-in-acceptance.md')
            ui.click('Use as new path')
            ui.label('Opened notes/created-in-acceptance.md')
            assert content() == conflicting
            ui.click('Save'); ui.label('Save Markdown to GitHub'); ui.click('Approve & Save')
            ui.label('GitHub committed')
            ui.wait(lambda: not ui.find(text='Save Markdown to GitHub', kind='Label'))
            remote = json.loads(provider.read_text())
            assert remote['revision'] == 2
            assert remote['files']['notes/created-in-acceptance.md']['content'] == conflicting
            ui.capture('06-new-file-commit')
            receipt['checks'].append('Explicit new Markdown path creates a reviewed file through actual host API')
            uncertain = conflicting + '\nAn uncertain response must not trigger an automatic duplicate save.\n'
            ui.field('markdown', uncertain)
            control.write_text(json.dumps({'next_save': 'uncertain'}))
            ui.click('Save'); ui.label('Save Markdown to GitHub'); ui.click('Approve & Save')
            ui.label('Synthetic connection failed after storing', identifier='connector_review_status')
            assert ui.find(text='Approve & Save') is None, 'Consumed approval remains clickable'
            ui.capture('07-uncertain-save-response')
            remote = json.loads(provider.read_text())
            assert remote['revision'] == 3
            assert remote['files']['notes/created-in-acceptance.md']['content'] == uncertain
            ui.click('Back to editing')
            assert content() == uncertain
            ui.wait(lambda: persisted().get('content') == uncertain and persisted()['dirty'])
            receipt['checks'].append('Lost provider response retains dirty draft and consumed approval cannot retry automatically')
            conflicting = uncertain
            ui.check_logs(); ui.close(); ui = None
            control.write_text(json.dumps({'offline':True}))
            ui = start('03-offline-restart'); ui.tab('Markdown')
            assert content() == conflicting
            ui.click('Repository'); ui.click('Refresh')
            ui.label('Synthetic provider is offline'); ui.click('Back to note')
            assert content() == conflicting
            ui.capture('08-offline-restart-retains-note')
            receipt['checks'].append('Unsent text and destination survive provider-offline installed restart')
            ui.check_logs()
            receipt['actions'] = [{'session': session.name, 'actions': session.actions} for session in sessions]
            receipt['provider_requests'] = [json.loads(line) for line in (provider.parent/'github-requests.jsonl').read_text().splitlines()]
            assert len([r for r in receipt['provider_requests'] if r['method'] == 'PUT']) == 4, 'Unexpected write retry'
            receipt['synthetic_provider_final_revision'] = json.loads(provider.read_text())['revision']
            assert all(digest(p)==hashes[str(p.relative_to(workspace.parent))] for p in sources), 'Sources changed during acceptance'
            receipt['result'] = 'pass'
        except BaseException as error:
            receipt['result'] = 'fail'; receipt['failure'] = str(error)
            if ui:
                try: ui.capture('failure')
                except Exception: pass
            raise
        finally:
            if ui: ui.close()
            receipt['actions'] = [{'session': session.name, 'actions': session.actions} for session in sessions]
            receipt['captures'] = {p.name:digest(p) for p in evidence.glob('*.png')}
            (evidence/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
            print('Evidence:',evidence)
    print('PASS: installed Notes flow with synthetic provider; native pixel review pending')


if __name__ == '__main__':
    run()
