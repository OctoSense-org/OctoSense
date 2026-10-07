#!/usr/bin/env python3
"""Exercise the real desktop browser adapter against a disposable local page.

Requires an already built octosense-browser-smoke and the platform's installed
browser runtime. This never downloads or installs a runtime, opens personal
accounts, or uses a simulated renderer. Keep the output directory private.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time


HTML = b'''<!doctype html><meta charset="utf-8"><title>Embedded browser fixture</title>
<style>body{font:18px system-ui;background:#eef4fb;color:#17304b;padding:32px}
input,button{font:inherit;padding:12px}article{max-width:560px}p{line-height:1.6}</style>
<article><h1>Embedded browser fixture</h1><p>A real native browser renders this page.
It contains no account, password or personal information.</p>
<input id="message" aria-label="Message"><button id="submit">Save locally</button>
<p id="result">Ready</p></article><script>
function report(kind,extra={}){fetch(location.href,{method:'POST',headers:{'Content-Type':'application/json'},
body:JSON.stringify({kind,width:innerWidth,height:innerHeight,
bridge:typeof window.octos_native!=='undefined'||typeof window.octos!=='undefined'||!!(window.chrome&&chrome.webview),...extra})}).catch(()=>{})}
document.querySelector('#submit').onclick=()=>{let value=document.querySelector('#message').value;
document.querySelector('#result').textContent=value;document.title='Saved: '+value;report('edited',{value})};
report('loaded');setInterval(()=>report('heartbeat'),250);
</script>'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--host-arg', action='append', default=[])
    parser.add_argument('--require-snapshot', action='store_true')
    parser.add_argument('--require-xembed', action='store_true', help='Probe geometry on an isolated, owned Linux X display only')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = args.out.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    requests = []
    requests_lock = threading.Lock()
    checks = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            with requests_lock:
                requests.append({'method': 'GET', 'path': self.path,
                                 'cookie_present': bool(self.headers.get('Cookie'))})
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(HTML)))
            self.send_header('Set-Cookie', 'fixture=synthetic; HttpOnly; SameSite=Strict; Path=/')
            self.end_headers()
            self.wfile.write(HTML)

        def do_POST(self):
            length = min(int(self.headers.get('Content-Length', 0)), 4096)
            try:
                body = json.loads(self.rfile.read(length))
            except (ValueError, OSError):
                body = {'invalid': True}
            with requests_lock:
                requests.append({'method': 'POST', 'path': self.path, 'body': body,
                                 'cookie_present': bool(self.headers.get('Cookie'))})
            self.send_response(204)
            self.end_headers()

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    page = f'http://127.0.0.1:{server.server_port}/page'
    env = os.environ.copy()
    env['MAKEPAD_HIDE_WINDOWS'] = '1'
    process = None
    sequence = 0
    failure = None

    def events():
        path = root / 'events.jsonl'
        if not path.is_file():
            return []
        result = []
        for line in path.read_text(encoding='utf-8').splitlines():
            try:
                result.append(json.loads(line))
            except ValueError:
                pass  # A writer can be between write() and its newline.
        return result

    def wait(predicate, label, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            if process is not None and process.poll() is not None:
                raise AssertionError(f'Native host exited during {label} (status {process.returncode})')
            time.sleep(.1)
        raise AssertionError(f'Timed out: {label}')

    def command(operation, **fields):
        nonlocal sequence
        sequence += 1
        staged = root / 'command.next'
        staged.write_text(json.dumps({'id': sequence, 'op': operation, **fields}), encoding='utf-8')
        staged.replace(root / 'command.json')
        wait(lambda: any(e.get('kind') == 'command' and e.get('id') == sequence for e in events()), operation)
        assert any(e.get('id') == sequence and e.get('accepted') for e in events()), operation
        return sequence

    def server_events():
        with requests_lock:
            return list(requests)

    try:
        with (root / 'native.log').open('wb') as log:
            process = subprocess.Popen([str(binary), f'--control-root={root}', '--remote', *args.host_arg],
                                       env=env, stdout=log, stderr=subprocess.STDOUT)
            wait(lambda: any(e.get('kind') == 'started' for e in events()), 'native host startup')
            command('open', url=page)
            wait(lambda: any(e.get('body', {}).get('kind') == 'loaded' for e in server_events()), 'real document JavaScript')
            loaded = next(e for e in server_events() if e.get('body', {}).get('kind') == 'loaded')
            assert loaded['body']['width'] > 100 and loaded['body']['height'] > 100, loaded['body']
            assert not loaded['body']['bridge'], 'Ordinary page received a native bridge'
            assert loaded['cookie_present'], 'Browser did not retain its own HttpOnly fixture cookie'
            checks.append('native_document_loaded_without_bridge')
            if args.require_xembed:
                from browser_smoke_x11 import inspect_embedding
                (root / 'embedding.json').write_text(json.dumps(inspect_embedding(), indent=2) + '\n', encoding='utf-8')
                checks.append('real_xembed_child_hierarchy')

            command('eval', script="document.querySelector('#message').value='Reviewed locally';document.querySelector('#submit').click()")
            wait(lambda: any(e.get('body', {}).get('value') == 'Reviewed locally' for e in server_events()), 'real DOM edit and form action')
            checks.append('real_dom_edit_and_action')

            if args.require_snapshot:
                capture = command('inspect')
                wait(lambda: (root / f'inspect-{capture}.json').is_file(), 'native engine snapshot')
                meta = json.loads((root / f'inspect-{capture}.json').read_text(encoding='utf-8'))
                assert not meta.get('error'), meta.get('error')
                assert meta.get('title') == 'Saved: Reviewed locally'
                assert meta.get('viewportWidth', 0) > 100 and meta.get('viewportHeight', 0) > 100
                assert not meta.get('hostBridgePresent'), 'Inspection found a native bridge'
                snapshot = (root / f'snapshot-{capture}.png').read_bytes()
                assert len(snapshot) > 100 and snapshot.startswith(bytes([137, 80, 78, 71, 13, 10, 26, 10]))
                checks.append('native_engine_snapshot_saved')

            before = len(events())
            command('eval', script="location.href='/forbidden'")
            wait(lambda: any(e.get('kind') == 'page_error' for e in events()[before:]), 'restricted navigation rejection')
            assert not any(e['path'] == '/forbidden' for e in server_events()), 'Forbidden navigation reached server'
            checks.append('restricted_navigation_blocked_before_request')
            command('hide')
            command('show')
            checks.append('detach_and_reattach_commands_accepted')
            command('close')
            time.sleep(.7)
            stopped = len(server_events())
            time.sleep(1)
            assert len(server_events()) == stopped, 'Closed native page kept making requests'
            checks.append('close_stops_page_execution')

            before = len(server_events())
            command('open', url=page)
            wait(lambda: any(e.get('body', {}).get('kind') == 'loaded' for e in server_events()[before:]), 'fresh browser after close')
            reopened = next(e for e in server_events()[before:] if e['method'] == 'GET' and e['path'] == '/page')
            assert not reopened['cookie_present'], 'Reopened view reused the previous profile'
            checks.append('reopen_uses_fresh_profile')

            with socket.socket() as unused:
                unused.bind(('127.0.0.1', 0))
                closed_port = unused.getsockname()[1]
            before = len(events())
            command('open', url=f'http://127.0.0.1:{closed_port}/unavailable')
            wait(lambda: any(e.get('kind') == 'page_error' for e in events()[before:]), 'native network failure')
            checks.append('native_network_error_reported')
            command('quit')
            process.wait(timeout=10)
            assert process.returncode == 0
    except Exception as error:
        failure = str(error)
    finally:
        if process is not None and process.poll() is None:
            try:
                command('quit')
                process.wait(timeout=5)
            except Exception:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
        receipt = {'schema': 1, 'platform': os.name, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                   'checks': checks, 'passed': failure is None, 'failure': failure,
                   'scope': 'real native engine; DOM actions are automated, not physical typing or visual UX approval',
                   'owned_process_exited': process is None or process.poll() is not None}
        (root / 'requests.json').write_text(json.dumps(server_events(), indent=2) + '\n', encoding='utf-8')
        (root / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(receipt, indent=2))
    return 1 if failure else 0


if __name__ == '__main__':
    raise SystemExit(main())
