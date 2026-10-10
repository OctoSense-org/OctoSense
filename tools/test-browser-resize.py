#!/usr/bin/env python3
"""macOS native WebReader resize/frozen-capture regression; local fictional page only."""
import argparse
import http.server
import json
from pathlib import Path
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/connected-e2e'))
from native import Native

PAGE = b'''<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Native reader resize fixture</title>
<style>*{box-sizing:border-box}body{margin:0;background:#f6f3fc;font:16px system-ui}
main{max-width:480px;margin:auto;padding:24px}input{width:100%;padding:12px}</style>
<main><h1>Reader resize fixture</h1><label>Fictional draft<input id="draft"></label>
<p id="status"></p></main><script>
const loads=Number(sessionStorage.loads||0)+1;sessionStorage.loads=loads;
function report(){document.querySelector('#status').textContent=
'loads='+loads+' draft='+document.querySelector('#draft').value+' viewport='+innerWidth+'x'+innerHeight}
addEventListener('resize',report);document.querySelector('#draft').addEventListener('input',report);report();setTimeout(()=>{document.querySelector('#draft').value='retained fictional draft';report()},200);
</script>'''


class PageHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.end_headers()
        self.wfile.write(PAGE)

    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/octosense-browser-smoke')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This regression checks macOS native attachment; other platforms are unverified.')
    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=False)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), PageHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    native = None
    sequence = 0
    checks = []
    try:
        native = Native(args.binary.resolve(), ['--control-root=' + str(root), '--texture-surface'], root, 'resize-host')
        native.wait(lambda: (root / 'events.jsonl').is_file())

        def command(operation, **values):
            nonlocal sequence
            sequence += 1
            temporary = root / 'command.next'
            temporary.write_text(json.dumps({'id': sequence, 'op': operation, **values}))
            temporary.replace(root / 'command.json')
            def completed():
                # The host appends each record in multiple writes. Wait for its newline.
                for line in (root / 'events.jsonl').read_text().split('\n')[:-1]:
                    event = json.loads(line)
                    if event.get('kind') == 'command' and event.get('id') == sequence:
                        if not event['accepted']:
                            raise AssertionError('Fixture command rejected: ' + operation)
                        return True
                return False
            native.wait(completed)
            return sequence

        def inspect():
            number = command('inspect')
            path = root / f'inspect-{number}.json'
            def result():
                try:
                    return json.loads(path.read_text())
                except (FileNotFoundError, json.JSONDecodeError):
                    return None
            return native.wait(result)

        def loaded():
            state = inspect()
            return state if state.get('readyState') == 'complete' and state.get('title') == 'Native reader resize fixture' and 'retained fictional draft' in state.get('text', '') else None

        def current_size():
            state = inspect()
            row = native.find(identifier='browser', kind='WebReader')
            if not row or not state.get('nativeAttached'):
                return None
            width, height = native.call('s')['w'][0]['sz']
            if abs(row['r'][2] - (width - 32)) > 1 or abs(row['r'][1] + row['r'][3] - (height - 16)) > 1:
                return None
            if abs(state['viewportWidth'] - row['r'][2]) > 1 or abs(state['viewportHeight'] - row['r'][3]) > 1:
                return None
            assert 'loads=1 draft=retained fictional draft' in state['text'], state
            return state

        command('open', url=f'http://127.0.0.1:{server.server_port}/')
        native.wait(loaded)
        for width, height in [(1000, 700), (430, 850), (360, 400), (800, 620)]:
            command('resize', width=width, height=height)
            native.wait(lambda: native.call('s')['w'][0]['sz'] == [width, height])
            state = native.wait(current_size)
            checks.append({'window': [width, height], 'viewport': [state['viewportWidth'], state['viewportHeight']], 'attached': True})
            number = command('inspect')
            native.wait(lambda: (root / f'snapshot-{number}.png').is_file())
            native.capture(f'host-{width}-{height}')

        command('freeze')
        # The watchdog must hide even after the texture snapshot detaches its pass.
        native.wait(lambda: not inspect().get('nativeAttached'), timeout=5)
        command('resize', width=430, height=850)
        native.wait(lambda: native.call('s')['w'][0]['sz'] == [430, 850])
        time.sleep(.6)
        assert inspect()['nativeAttached'] is False
        checks.append({'frozen_after_resize': 'detached'})
        command('restore')
        restored = native.wait(current_size)
        checks.append({'restored_viewport': [restored['viewportWidth'], restored['viewportHeight']], 'draft_and_page_retained': True})
        native.check_logs()
        assert '[E]' not in native.log_path.read_text(), 'Native script error; inspect resize-host.log'
        (root / 'result.json').write_text(json.dumps({'result': 'passed', 'checks': checks}, indent=2) + '\n')
        print(json.dumps({'result': 'passed', 'checks': checks}, indent=2))
    finally:
        if native:
            native.close()
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


if __name__ == '__main__':
    main()
