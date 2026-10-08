"""Small W3C WebDriver adapter for the synthetic backend acceptance fixture.

Uses an already-running isolated WebDriver. It neither installs a browser nor
attaches to an existing user profile. Session URLs and provider responses are
never printed; the owning driver keeps failures in its private run directory.
"""
import base64
import fnmatch
import json
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request


class automation:
    def __init__(self, endpoint, browser):
        parsed = urllib.parse.urlsplit(endpoint)
        if parsed.scheme != 'http' or parsed.hostname not in ('127.0.0.1', 'localhost'):
            raise ValueError('WebDriver must be on loopback')
        self.endpoint = endpoint.rstrip('/')
        self.browser = browser
        self.session = None
        self.chromium = self

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def request(self, method, path, data=None):
        payload = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(self.endpoint + path, data=payload,
            headers={'Content-Type': 'application/json'}, method=method)
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                body = json.load(response)
        except urllib.error.HTTPError as error:
            body = json.load(error)
        value = body.get('value')
        if isinstance(value, dict) and 'error' in value:
            raise RuntimeError('WebDriver command failed: ' + value['error'])
        return value

    def call(self, method, path, data=None):
        return self.request(method, '/session/' + self.session + path, data)

    def launch(self, **_):
        caps = {'browserName': self.browser}
        if self.browser == 'MiniBrowser':
            caps['webkitgtk:browserOptions'] = {
                'binary': '/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser',
                'args': ['--automation', '--private']}
        else:
            caps['ms:edgeOptions'] = {'args': ['--inprivate', '--headless=new']}
        result = self.request('POST', '/session', {'capabilities': {'alwaysMatch': caps}})
        self.session = result['sessionId']
        self.call('POST', '/timeouts', {'pageLoad': 30000, 'script': 10000, 'implicit': 0})
        return self

    def new_context(self, viewport):
        self.call('POST', '/window/rect', viewport)
        return self

    def new_page(self):
        return self

    def on(self, *_):
        # W3C WebDriver has no portable browser-console subscription. HTTP
        # events and native logs remain checked by the acceptance driver.
        pass

    def goto(self, url):
        self.call('POST', '/url', {'url': url})

    def locator(self, selector):
        return _Element(self, selector)

    def screenshot(self, path):
        Path(path).write_bytes(base64.b64decode(self.call('GET', '/screenshot'), validate=True))

    def wait_for_url(self, pattern):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if fnmatch.fnmatchcase(self.call('GET', '/url'), pattern):
                return
            time.sleep(.1)
        raise AssertionError('Browser did not reach the native callback')

    def close(self):
        if self.session:
            try:
                self.call('DELETE', '')
            finally:
                self.session = None


class _Element:
    def __init__(self, driver, selector):
        self.driver = driver
        self.selector = selector

    def path(self):
        value = self.driver.call('POST', '/element', {'using': 'css selector', 'value': self.selector})
        identifier = value['element-6066-11e4-a52e-4f735466cecf']
        return '/element/' + urllib.parse.quote(identifier, safe='')

    def fill(self, text):
        path = self.path()
        self.driver.call('POST', path + '/clear', {})
        self.driver.call('POST', path + '/value', {'text': text})

    def click(self):
        self.driver.call('POST', self.path() + '/click', {})

    def inner_text(self):
        return self.driver.call('GET', self.path() + '/text')
