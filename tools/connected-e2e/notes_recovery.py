#!/usr/bin/env python3
"""A rejected saved draft must never become an editable blank replacement."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
from notes_native import NotesNative

APP = 'org.octosense.samples.githubnotes'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--host', type=Path, default=Path('target/release/examples/connected-app-host'))
    parser.add_argument('--installer', type=Path, default=Path('target/release/examples/connected-install'))
    parser.add_argument('--out', type=Path, default=Path('target/connected-notes-recovery'))
    args = parser.parse_args()
    args.bundle, args.host, args.installer = (p.resolve() for p in (args.bundle, args.host, args.installer))
    output = args.out / ('run-' + str(time.time_ns()))
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[2]
    sources = [Path(__file__).resolve(), Path(__file__).with_name('native.py'),
               Path(__file__).with_name('notes_native.py'), root/'crates/markdown-editor/src/lib.rs',
               args.bundle/'main.splash', args.bundle/'manifest.json']
    hashes = {str(p.relative_to(root.parent)): sha(p) for p in sources}
    receipt = {'result': 'running', 'scope': 'signed installed Notes rejected-draft recovery, fictional oversized data',
               'binary_sha256': sha(args.host), 'source_sha256': hashes, 'live_provider': False,
               'checks': [], 'visual_review': 'pending'}
    ui = None
    with tempfile.TemporaryDirectory(prefix='notes-rejected-draft-') as directory:
        profile = Path(directory)/'apps'
        jail = profile/APP
        large = '# Oversized fictional draft\n\n' + 'x' * (512 * 1024)
        original = {'connection': '', 'owner': '', 'repo': '', 'branch': 'main',
                    'path': 'notes/new-note.md', 'sha': None, 'content': large,
                    'dirty': True, 'message': 'Update note'}
        def newest():
            return max((json.loads(p.read_text()) for p in jail.glob('draft-*.json')), key=lambda d: d['revision'])['draft']
        try:
            subprocess.run([str(args.installer), '--keep-profile='+str(profile), str(args.bundle)],
                           check=True, capture_output=True, text=True)
            jail.mkdir(exist_ok=True)
            for revision, name in [(1, 'draft-a.json'), (2, 'draft-b.json')]:
                (jail/name).write_text(json.dumps({'revision': revision, 'draft': original}))
            ui = NotesNative(args.host, ['--installed-app='+APP, '--app-data='+str(profile),
                                        '--provider-fixture=github'], output, 'rejected-reload')
            ui.label('size limits')
            assert ui.find(identifier='markdown', kind='TextInput') is None, 'Rejected draft exposes an editable blank document'
            ui.label('Repository & file')
            ui.click('Back to note')
            ui.label('Repository & file')
            assert ui.find(identifier='markdown', kind='TextInput') is None
            assert newest()['content'] == large
            ui.capture('01-rejected-draft-retained')
            receipt['checks'].append('Rejected reload routes to recovery/settings; Back cannot expose blank editor; original draft remains exact')
            ui.click('Refresh'); ui.click('fixture-author/notes')
            ui.click('docs'); ui.click('docs/second-note.md')
            ui.click('Keep current draft')
            assert newest()['content'] == large
            ui.click('docs/second-note.md'); ui.click('Open & keep recovery')
            ui.label('Opened docs/second-note.md')
            assert json.loads((jail/'recovery.json').read_text())['content'] == large
            normal = '# Recovered workspace\n\nNew edits are deliberate.\n'
            ui.field('markdown', normal)
            ui.wait(lambda: newest()['content'] == normal)
            assert json.loads((jail/'recovery.json').read_text())['content'] == large
            ui.capture('02-explicit-replacement-retains-recovery')
            receipt['checks'].append('Explicit valid-file replacement preserves exact oversized recovery; new editing works afterward')
            requests = [json.loads(line) for line in (profile/'.host/fixtures/github-requests.jsonl').read_text().splitlines()]
            assert not any(r['method'] in ('PUT', 'POST', 'PATCH', 'DELETE') for r in requests)
            ui.check_logs()
            assert all(sha(p) == hashes[str(p.relative_to(root.parent))] for p in sources)
            receipt['provider_write_attempts'] = 0
            receipt['result'] = 'pass'
        except BaseException as error:
            receipt['result'] = 'fail'; receipt['failure'] = str(error)
            if ui:
                try: ui.capture('failure')
                except Exception: pass
            raise
        finally:
            if ui:
                ui.close()
                receipt['exit_code'] = ui.child.returncode
            receipt['captures'] = {p.name: sha(p) for p in output.glob('*.png')}
            (output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
            print('Evidence:', output)


if __name__ == '__main__':
    run()
