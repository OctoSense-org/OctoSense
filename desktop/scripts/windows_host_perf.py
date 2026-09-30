#!/usr/bin/env python3
"""Compare hidden Windows host redraws, CPU and input using a cached Terminal.

Pass --binary, --terminal and --output as paths. Each run isolates app data and
launches the supplied binaries directly, keeping compilation outside the sample.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
from urllib.parse import urlencode
from urllib.request import urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--terminal', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=12)
    parser.add_argument('--count', type=int, default=1)
    parser.add_argument('--lifecycle', action='store_true')
    args = parser.parse_args()
    if os.name != 'nt':
        parser.error('Windows is required')
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    trace = out / 'host.log'
    if trace.exists():
        parser.error('Use a fresh output directory')
    home = out / 'home'
    (home / 'terminal').mkdir(parents=True, exist_ok=True)
    (home / 'terminal/settings.conf').write_text('shell = pwsh.exe\ncursor-blink = no\n')
    catalog = out / 'apps.json'
    catalog.write_text(json.dumps([{'id': 'terminal', 'label': 'Terminal',
                                   'executable': str(args.terminal.resolve()), 'policy': 'new'}]))
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, MAKEPAD_HIDE_WINDOWS='1', MAKEPAD_REMOTE=str(port),
               MAKEPAD_WM_TRACE=str(trace), MAKEPAD_WM_TRACE_BLUR='1',
               MAKEPAD_WM_TEST_APP=f'terminal:{args.count}', MAKEPAD_DEVTOOLS='0',
               OCTOSENSE_HOME=str(home), OCTOSENSE_APP_DATA=str(home / 'apps'),
               MAKEPAD_HOME=str(home))
    env.pop('OCTOS_APP_CORE_BIN', None)

    def get(route, **params):
        with urlopen(f'http://127.0.0.1:{port}/{route}?' + urlencode(params), timeout=20) as reply:
            value = json.load(reply)
        if isinstance(value, dict) and 'err' in value:
            raise RuntimeError(value['err'])
        return value

    kernel = c.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    kernel.OpenProcess.restype = w.HANDLE
    kernel.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
    kernel.CloseHandle.argtypes = [w.HANDLE]

    def cpu(handle):
        times = [w.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(handle, *[c.byref(t) for t in times]):
            raise c.WinError(c.get_last_error())
        return sum((t.dwHighDateTime << 32) + t.dwLowDateTime for t in times[2:]) / 1e7

    with (out / 'stdout.log').open('w') as stdout, (out / 'stderr.log').open('w') as stderr:
        process = subprocess.Popen([str(args.binary.resolve()), '--apps', str(catalog)],
                                   cwd=root, env=env, stdout=stdout, stderr=stderr,
                                   creationflags=subprocess.CREATE_NO_WINDOW)
        handle = kernel.OpenProcess(0x1000, False, process.pid)
        result = {'pid': process.pid, 'binary': str(args.binary.resolve()),
                  'terminal': str(args.terminal.resolve()), 'count': args.count}
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f'Host exited: {process.returncode}')
                if trace.exists() and all(f' H pd c{i}' in trace.read_text() for i in range(1, args.count + 1)):
                    break
                time.sleep(.1)
            else:
                raise TimeoutError('Terminal did not present')
            time.sleep(5)
            before = len(trace.read_text().splitlines())
            start_cpu, start = cpu(handle), time.perf_counter()
            time.sleep(args.seconds)
            elapsed, used = time.perf_counter() - start, cpu(handle) - start_cpu
            lines = trace.read_text().splitlines()[before:]
            result.update(seconds=elapsed, cpu_one_core_percent=100 * used / elapsed,
                          paints=sum(' H paint ' in x for x in lines),
                          child_frames=sum(' H pd ' in x for x in lines),
                          ticks=sum(' H tick ' in x for x in lines),
                          acknowledgements=sum(' H ack ' in x for x in lines))
            result['state'] = get('s')
            snap = get('snap', q='MpRunView')
            result['views'] = snap
            views = [v for v in snap.get('s', []) if v.get('r', [0, 0, 0, 0])[2] > 0]
            if not views:
                raise RuntimeError('No hosted view to exercise')
            x, y, width, height = views[-1]['r']
            get('click', x=x + width / 2, y=y + height / 2, wait=1)
            # Type without Enter: the test never executes a terminal command.
            started = time.perf_counter()
            get('t', t='OCTOSENSE_PACING_CHECK', wait=1)
            result['text_dispatch_ms'] = 1000 * (time.perf_counter() - started)
            time.sleep(.6)
            shot = get('g', scale=.5)
            shutil.copyfile(shot['png'], out / 'typed.png')
            # Exercise coalesced moves and a selection's button edges.
            get('m', k='down', x=x + 20, y=y + height / 2)
            for i in range(30):
                get('m', k='move', x=x + 20 + i * 3, y=y + height / 2)
            get('m', k='up', x=x + 107, y=y + height / 2, wait=1)
            result['pointer_edges'] = 'completed'
            if args.lifecycle:
                user = c.WinDLL('user32', use_last_error=True)
                callback_type = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
                user.EnumWindows.argtypes = [callback_type, w.LPARAM]
                user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
                user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
                user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
                user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
                windows = []

                @callback_type
                def collect(hwnd, _):
                    owner = w.DWORD()
                    user.GetWindowThreadProcessId(hwnd, c.byref(owner))
                    title = c.create_unicode_buffer(256)
                    user.GetWindowTextW(hwnd, title, len(title))
                    if owner.value == process.pid and title.value.startswith('OctoSense'):
                        windows.append(hwnd)
                    return True

                user.EnumWindows(collect, 0)
                if not windows:
                    raise RuntimeError('Missing hidden native window')
                hwnd = windows[0]
                rect = w.RECT()
                user.GetWindowRect(hwnd, c.byref(rect))
                original = get('s')['w'][0]['sz']
                # Keep the test window hidden and preserve focus and stacking.
                if not user.SetWindowPos(hwnd, None, 0, 0, 2200, 1400, 0x16):
                    raise c.WinError(c.get_last_error())
                time.sleep(1)
                resized = get('s')['w'][0]['sz']
                if resized == original:
                    raise RuntimeError('Window did not resize')
                shot = get('g', scale=.5)
                shutil.copyfile(shot['png'], out / 'resized.png')
                user.SetWindowPos(hwnd, None, 0, 0, rect.right - rect.left, rect.bottom - rect.top, 0x16)
                time.sleep(1)
                result['resize'] = {'original': original, 'resized': resized, 'restored': get('s')['w'][0]['sz']}
                # Exercise an unexpected child exit, only in this isolated host.
                query = f'Get-CimInstance Win32_Process -Filter "ParentProcessId={process.pid}" | Select-Object ProcessId,ExecutablePath | ConvertTo-Json -Compress'
                children = json.loads(subprocess.check_output(['powershell', '-NoProfile', '-Command', query],
                                      creationflags=subprocess.CREATE_NO_WINDOW, text=True))
                if isinstance(children, dict):
                    children = [children]
                child = next(child for child in children if child.get('ExecutablePath')
                             and Path(child['ExecutablePath']).resolve() == args.terminal.resolve())
                child_handle = kernel.OpenProcess(1, False, child['ProcessId'])
                kernel.TerminateProcess.argtypes = [w.HANDLE, w.UINT]
                if not child_handle or not kernel.TerminateProcess(child_handle, 1):
                    raise c.WinError(c.get_last_error())
                kernel.CloseHandle(child_handle)
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    if 'stopped; its tile stays closed with Restart' in (out / 'stdout.log').read_text(encoding='utf-8', errors='replace'):
                        break
                    time.sleep(.2)
                else:
                    raise RuntimeError('Stopped child did not show restart panel')
                time.sleep(1)
                shot = get('g', scale=.5)
                shutil.copyfile(shot['png'], out / 'stopped.png')
                before_restart = trace.read_text().count(' H pd ')
                current_views = get('snap', q='MpRunView').get('s', [])
                view = current_views[-1]['r'] if current_views else [x, y, width, height]
                get('click', x=view[0] + view[2] / 2, y=view[1] + view[3] / 2, wait=1)
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    if trace.read_text().count(' H pd ') > before_restart:
                        break
                    time.sleep(.2)
                else:
                    raise RuntimeError('Restarted child did not present')
                result['restart'] = 'stopped panel shown; replacement child presented'
        finally:
            try:
                get('quit')
            except Exception:
                pass
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
            kernel.CloseHandle(handle)
            result['exit_code'] = process.returncode
            (out / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
