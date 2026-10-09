"""Owned hidden native process helpers. No provider or approval emulation."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import urllib.parse
import urllib.request

# Select-all is Cmd+A on macOS and Ctrl+A elsewhere.
SELECT_ALL = {'cmd': 1} if sys.platform == 'darwin' else {'ctrl': 1}


class Native:
    def __init__(self, binary, arguments, evidence, name):
        self.evidence = Path(evidence)
        self.name = name
        self.log_path = self.evidence / (name + '.log')
        self.log = self.log_path.open('w')
        self.child = subprocess.Popen([str(binary), *arguments, '--remote'],
                                     env={**os.environ, 'MAKEPAD_HIDE_WINDOWS': '1'},
                                     stdout=self.log, stderr=self.log)
        self.endpoint = None
        self.actions = []
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            match = re.search(r'listening on (127\.0\.0\.1:\d+)', self.log_path.read_text())
            if match:
                self.endpoint = 'http://' + match[1]
                break
            if self.child.poll() is not None:
                raise RuntimeError('Native process exited during startup; inspect ' + self.log_path.name)
            time.sleep(.1)
        if self.endpoint is None:
            self.close()
            raise RuntimeError('Native instrument did not become ready')

    def call(self, route, **query):
        url = self.endpoint + '/' + route + ('?' + urllib.parse.urlencode(query) if query else '')
        with urllib.request.urlopen(url, timeout=20) as response:
            result = json.load(response)
        if 'err' in result:
            raise RuntimeError(result['err'])
        return result

    def rows(self):
        return [r for r in self.call('snap')['s'] if r.get('ty') != 'Splash'
                and r.get('r', [0, 0, 0, 0])[2] > 0 and r['r'][3] > 0]

    def wait(self, predicate, timeout=12):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = predicate()
            if result:
                return result
            time.sleep(.08)
        raise AssertionError('Native state did not settle; inspect ' + self.name)

    def label(self, text, identifier=None):
        return self.wait(lambda: next((r for r in self.rows()
                                      if r.get('ty') in ('Label', 'LinkLabel')
                                      and text in r.get('t', '')
                                      and (identifier is None or r.get('i') == identifier)), None))

    def find(self, text=None, identifier=None, kind='Button'):
        candidates = [r for r in self.rows() if (kind is None or r.get('ty') == kind)
                      and (text is None or r.get('t') == text)
                      and (identifier is None or r.get('i') == identifier)]
        if len(candidates) > 1:
            raise AssertionError('Ambiguous widget selector: ' + str((text, identifier)))
        return candidates[0] if candidates else None

    def click_row(self, row):
        x, y, width, height = row['r']
        self.call('click', x=x + width / 2, y=y + height / 2, wait=1)
        self.actions.append({'click': row.get('t', row.get('i')), 'rect': row['r']})

    def reachable(self, text=None, identifier=None, kind='Button'):
        row = self.find(text, identifier, kind)
        size = self.call('s')['w'][0]['sz']
        def fits(r):
            return r and r['r'][1] >= 36 and r['r'][1] + r['r'][3] <= size[1] - 12 and r['r'][3] >= (8 if kind == 'Label' else 28)
        if fits(row):
            return row
        self.call('m', k='scroll', x=size[0] / 2, y=size[1] * .55, dy=-5000, wait=1)
        for _ in range(16):
            row = self.find(text, identifier, kind)
            if fits(row):
                return row
            self.call('m', k='scroll', x=size[0] / 2, y=size[1] * .55, dy=190, wait=1)
        raise AssertionError('Widget cannot be reached by scrolling: ' + str((text, identifier)))

    def click(self, text):
        self.click_row(self.reachable(text=text))

    def tab(self, text):
        if self.find(text='● ' + text):
            return
        self.click(text)

    def field(self, identifier, value):
        self.click_row(self.reachable(identifier=identifier, kind='TextInput'))
        self.call('k', k='press', c='KeyA', wait=1, **SELECT_ALL)
        self.call('t', t=value, wait=1)
        self.wait(lambda: (self.find(identifier=identifier, kind='TextInput') or {}).get('t') == value)
        self.actions.append({'edit': identifier, 'characters': len(value)})

    def capture(self, name):
        shutil.copyfile(Path(self.call('g')['png']), self.evidence / (name + '.png'))
        (self.evidence / (name + '.snapshot.json')).write_text(json.dumps(self.rows(), indent=2) + '\n')

    def check_logs(self):
        text = self.log_path.read_text()
        errors = [line for line in text.splitlines() if any(marker in line for marker in
                  ['"level":"error"', '[ERROR]', 'ScriptError', 'callback error', 'on_render closure failed', 'thread \'', 'panicked'])]
        if errors:
            raise AssertionError('Native hard errors; inspect ' + self.log_path.name + ': ' + '\n'.join(errors[:3]))

    def close(self):
        if self.child.poll() is None:
            if self.endpoint:
                try:
                    self.call('quit')
                except Exception:
                    pass
            try:
                self.child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.child.terminate()
                self.child.wait(timeout=5)
        self.log.close()
