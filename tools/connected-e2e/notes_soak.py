#!/usr/bin/env python3
"""Repeated native Notes UX against an isolated, signed synthetic-provider profile.

Instrument frame-wait round trips are not display latency or a phone benchmark.
No real provider credentials, repository writes or physical approval are used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time
import urllib.request
from notes_native import NotesNative as Native

APP = 'org.octosense.samples.githubnotes'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def text_sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def distribution(samples):
    ordered = sorted(samples)
    def percentile(p):
        position = (len(ordered) - 1) * p
        left = int(position)
        return ordered[left] + (ordered[min(left + 1, len(ordered) - 1)] - ordered[left]) * (position - left)
    return {'count': len(samples), 'p50_ms': round(percentile(.5), 2),
            'p95_ms': round(percentile(.95), 2), 'max_ms': round(max(samples), 2)} if samples else {}


def run():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--host', type=Path, default=Path('target/release/examples/connected-app-host'))
    parser.add_argument('--installer', type=Path, default=Path('target/release/examples/connected-install'))
    parser.add_argument('--cycles', type=int, default=36)
    parser.add_argument('--duration-seconds', type=float, default=600)
    parser.add_argument('--out', type=Path, default=Path('target/connected-notes-soak'))
    args = parser.parse_args()
    assert 1 <= args.cycles <= 500
    assert 0 <= args.duration_seconds <= 3600
    args.bundle, args.host, args.installer = (p.resolve() for p in (args.bundle, args.host, args.installer))
    root = Path(__file__).resolve().parents[2]
    output = args.out / ('run-' + str(time.time_ns()))
    output.mkdir(parents=True)
    sources = [Path(__file__).resolve(), Path(__file__).with_name('native.py'), Path(__file__).with_name('notes_native.py'), root/'Cargo.lock',
               root/'crates/markdown-editor/src/lib.rs', root/'crates/shell/examples/connected-app-host.rs',
               root/'crates/shell/examples/connected_support/mod.rs', root/'crates/oauth-service/src/host_api.rs',
               root/'crates/oauth-service/src/acceptance_github.rs', args.bundle/'main.splash', args.bundle/'manifest.json']
    sources += sorted((root/'crates/markdown-editor').glob('src/*.rs'))
    sources += sorted((root/'crates/markdown-editor/resources').rglob('*.svg'))
    sources = list(dict.fromkeys(sources))
    hashes = {str(p.relative_to(root.parent)): sha(p) for p in sources}
    receipt = {'result': 'running', 'scope': 'hidden native macOS signed installed Notes UX soak',
               'driver': 'Codex through Makepad instrument native events', 'cycles_requested': args.cycles,
               'binary_sha256': sha(args.host), 'installer_sha256': sha(args.installer),
               'source_sha256': hashes, 'cycles': [], 'metrics': {},
               'measurement_boundary': 'HTTP instrument round trips with wait=1, including native frame wait, IPC and polling; not presentation latency or continuous frame statistics. RSS includes native renderer, history and screenshot allocations.',
               'live_provider': False, 'physical_input': False, 'visual_review': 'pending', 'cleanup': 'pending'}
    timings = {}
    sessions = []
    ui = None
    started = time.monotonic()
    def timed(label, task):
        before = time.monotonic()
        result = task()
        elapsed = (time.monotonic() - before) * 1000
        timings.setdefault(label, []).append(elapsed)
        return result
    def rss():
        return int(subprocess.check_output(['ps', '-o', 'rss=', '-p', str(ui.child.pid)], text=True).strip())
    with tempfile.TemporaryDirectory(prefix='connected-notes-soak-') as directory:
        profile = Path(directory)/'apps'
        provider = profile/'.host/fixtures/github-state.json'
        def persist():
            drafts = [json.loads(p.read_text()) for p in (profile/APP).glob('draft-*.json')]
            return max(drafts, key=lambda d: d['revision']) if drafts else {}
        def exact(expected):
            ui.wait(lambda: persist().get('draft', {}).get('content') == expected)
            assert ui.find(identifier='markdown', kind='TextInput')['t'] == expected
            assert persist()['draft']['dirty']
        def start(name):
            instance = Native(args.host, ['--installed-app='+APP, '--app-data='+str(profile), '--provider-fixture=github'], output, name)
            sessions.append(instance)
            instance.label('Saved locally')
            assert 'CONNECTED_INSTALLED' in instance.log_path.read_text()
            return instance
        def scroll(dy):
            size = ui.call('s')['w'][0]['sz']
            ui.call('m', k='scroll', x=size[0]*.65, y=size[1]*.55, dy=dy, wait=1)
        def snapshot_receipt():
            receipt['metrics'] = {key: distribution(values) for key, values in timings.items()}
            receipt['elapsed_s'] = round(time.monotonic()-started, 3)
            (output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
        try:
            installed = subprocess.run([str(args.installer), '--keep-profile='+str(profile), str(args.bundle)], capture_output=True, text=True, check=True)
            receipt['installation'] = json.loads(installed.stdout)
            ui = start('01-soak')
            with urllib.request.urlopen(ui.endpoint+'/',timeout=10) as response:
                (output/'instrument-protocol.txt').write_bytes(response.read())
            ui.click('Repository')
            ui.wait(lambda: ui.find(text='Selected · Fixture GitHub · synthetic provider'))
            ui.click('Refresh repositories'); ui.label('Choose a repository')
            ui.click('fixture-author/notes'); ui.click('docs'); ui.click('docs/second-note.md')
            ui.label('Opened docs/second-note.md'); ui.tab('Markdown')
            receipt['rss_before_cycles_kib'] = rss()
            expected = ''
            soak_started = time.monotonic()
            for cycle in range(1, args.cycles+1):
                before = time.monotonic()
                lines = 48 if cycle % 2 == 0 else 8
                expected = f'# Unicode soak {cycle:02d}\n\nCafé, 谢谢, mañana, 日本語 — cycle {cycle}.\n\n'
                expected += '\n\n'.join(f'Paragraph {n:02d}: Keep appointment Wednesday 14:30; 编辑完成 café {cycle:02d}.' for n in range(1, lines+1))
                expected += f'\n\n## End marker {cycle:02d}\n\n| Task | Status |\n| --- | --- |\n| Unicode | Ready |\n\n```rust\nlet cycle = {cycle};\n```\n'
                timed('source_edit_and_persist', lambda: (ui.field('markdown', expected), exact(expected)))
                timed('source_scroll_and_refocus', lambda: (scroll(620), scroll(-5000), ui.click_row(ui.reachable(identifier='markdown', kind='TextInput'))))
                timed('write_mode', lambda: ui.tab('Write'))
                timed('write_scroll', lambda: (scroll(750), scroll(-5000)))
                if cycle == 12:
                    scroll(10000); ui.capture('long-write-scrolled-bottom'); scroll(-10000)
                if cycle % 6 == 0:
                    rows = [r for r in ui.rows() if r.get('ty')=='ArticleRichInput' and r.get('t')]
                    if not rows:
                        raise AssertionError('No native rich input available for edit/undo')
                    first = sorted(rows, key=lambda r:r['r'][1])[0]
                    x,y,w,h = first['r']
                    ui.call('click', x=x+min(w/2,100), y=y+min(h/2,12), wait=1)
                    ui.call('k', k='press', c='ArrowRight', cmd=1, wait=1)
                    marker = f' rich 编辑 {cycle:02d}'
                    ui.call('t', t=marker, wait=1)
                    ui.wait(lambda: marker in persist().get('draft',{}).get('content',''))
                    ui.call('k', k='press', c='KeyZ', cmd=1, wait=1)
                    ui.wait(lambda: persist().get('draft',{}).get('content') == expected)
                timed('preview_mode', lambda: ui.tab('Preview'))
                if cycle in (1,12,args.cycles): ui.capture(f'cycle-{cycle:02d}-preview-top')
                timed('preview_scroll', lambda: (scroll(10000), scroll(-10000)))
                if cycle == 12:
                    scroll(10000); ui.capture('long-preview-scrolled-bottom'); scroll(-10000)
                timed('return_source', lambda: ui.tab('Markdown'))
                exact(expected)
                timed('repository_and_back', lambda: (ui.click('Repository'), ui.label('Repository & file'), scroll(700), scroll(-5000), ui.click('Back to note')))
                exact(expected)
                if cycle % 6 == 0:
                    timed('exact_review_open', lambda: (ui.click('Save'), ui.label('Save Markdown to GitHub'), ui.label(expected)))
                    if cycle in (6,args.cycles): ui.capture(f'cycle-{cycle:02d}-exact-review')
                    timed('review_scroll_to_end', lambda: scroll(10000))
                    if cycle == args.cycles: ui.capture('final-review-scrolled-bottom')
                    timed('review_cancel', lambda: (ui.click('Back to editing'), ui.wait(lambda: not ui.find(text='Save Markdown to GitHub', kind='Label')), exact(expected)))
                    assert json.loads(provider.read_text())['revision'] == 0
                ui.check_logs()
                receipt['cycles'].append({'cycle':cycle, 'bytes':len(expected.encode()), 'content_sha256':text_sha(expected),
                                          'persisted_revision':persist()['revision'], 'rss_kib':rss(),
                                          'duration_ms':round((time.monotonic()-before)*1000,2),
                                          'rich_edit_undo':cycle%6==0,'review_cancel':cycle%6==0,'result':'pass'})
                idle_until = soak_started + args.duration_seconds * cycle / args.cycles
                idle_started = time.monotonic()
                while time.monotonic() < idle_until:
                    time.sleep(max(0,min(2, idle_until-time.monotonic())))
                    assert ui.child.poll() is None, 'Native process stopped during timer soak'
                    assert persist()['draft']['content'] == expected, 'Idle callback changed the draft'
                receipt['cycles'][-1]['idle_s'] = round(time.monotonic()-idle_started,3)
                receipt['cycles'][-1]['rss_after_idle_kib'] = rss()
                snapshot_receipt()
                print(f'Cycle {cycle}/{args.cycles}: exact draft retained; RSS {receipt["cycles"][-1]["rss_kib"]} KiB', flush=True)
            ui.capture('final-source-before-reopen')
            ui.close(); ui = None
            ui = timed('installed_reopen', lambda: start('02-reopen'))
            ui.tab('Markdown'); exact(expected); ui.capture('final-reopen-retained-source')
            receipt['reopen_exact_sha256'] = text_sha(expected)
            requests = [json.loads(line) for line in (provider.parent/'github-requests.jsonl').read_text().splitlines()]
            assert not any(r['method'] in ('PUT','POST','PATCH','DELETE') for r in requests)
            receipt['provider_write_attempts'] = 0
            ui.check_logs()
            assert all(sha(p)==hashes[str(p.relative_to(root.parent))] for p in sources), 'Sources changed during soak'
            samples = [row['rss_kib'] for row in receipt['cycles']]
            steady = samples[5:] or samples
            xs = list(range(len(steady)))
            meanx = statistics.mean(xs); meany = statistics.mean(steady)
            slope = sum((x-meanx)*(y-meany) for x,y in zip(xs,steady))/sum((x-meanx)**2 for x in xs) if len(xs)>1 else 0
            receipt['rss_trend'] = {'first_kib':samples[0],'last_kib':samples[-1],'max_kib':max(samples),
                                    'last5_median_kib':statistics.median(samples[-5:]),
                                    'first5_median_kib':statistics.median(samples[:5]),
                                    'after5_warmup_slope_kib_per_cycle':round(slope,2),
                                    'interpretation':'Measured finite-run trend only; history and renderer allocations are not a proven leak or leak-free guarantee.'}
            receipt['result']='pass'
        except BaseException as error:
            receipt['result']='fail'; receipt['failure']=str(error)
            if ui:
                try: ui.capture('failure')
                except Exception: pass
            raise
        finally:
            if ui: ui.close()
            receipt['cleanup']='all owned native processes exited; isolated profile removed on completion'
            receipt['exit_codes']=[session.child.returncode for session in sessions]
            receipt['actions']=[{'session':session.name,'actions':session.actions} for session in sessions]
            receipt['captures']={p.name:sha(p) for p in output.glob('*.png')}
            snapshot_receipt()
            print('Evidence:',output,flush=True)
    print('PASS: native Notes soak; inspect original PNGs and performance trend separately')


if __name__=='__main__':
    run()
