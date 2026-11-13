#!/usr/bin/env python3
"""Check in-place Photos album naming with a disposable library."""
import argparse
import json
import os
from pathlib import Path
import time

from capture import AppNative, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()

    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-photos']

    def start(name):
        app = AppNative(args.binary.resolve(), actions, out, name)
        app.wait(lambda: app.find(text='Family', kind='Button'), timeout=90)
        time.sleep(1)
        return app

    def key(app, code, **modifiers):
        app.call('k', k='press', c=code, wait=1, **modifiers)
        time.sleep(.3)

    def album(app, title):
        return app.wait(lambda: app.find(text=title, kind='Button'), timeout=20)

    def naming(app):
        return app.wait(lambda: app.find(identifier='side_name', kind='TextInput'), timeout=15)

    def rename(app, title):
        x, y, w, h = album(app, title)['r']
        app.call('m', k='move', x=x+w/2, y=y+h/2, wait=1)
        app.call('m', k='click', b=1, x=x+w/2, y=y+h/2, wait=1)
        item = app.wait(lambda: ([r for r in app.rows() if r.get('ty') == 'PopupMenuItem'
            and r.get('t') == 'Rename Album'] or [None])[-1], timeout=10)
        app.click_row(item)
        naming(app)
        time.sleep(.6)

    app = start('photos')
    try:
        key(app, 'KeyN', cmd=1)
        naming(app)
        time.sleep(.6)
        app.call('t', t='Café 🐙', wait=1)
        key(app, 'ReturnKey')
        album(app, 'Café 🐙')

        rename(app, 'Café 🐙')
        app.capture('selected-name')
        key(app, 'ArrowRight')
        key(app, 'ArrowLeft')
        app.call('t', t='!', wait=1)
        field = naming(app)
        assert field.get('val', field.get('t', '')) == 'Café !🐙', field
        app.wait(lambda: app.find(text='Select photos in your library, then choose Image > Add to “Café !🐙”.', kind='Label'))
        app.capture('edited-name')
        key(app, 'ReturnKey')
        album(app, 'Café !🐙')

        rename(app, 'Café !🐙')
        app.call('t', t='Temporary', wait=1)
        key(app, 'Escape')
        album(app, 'Café !🐙')
        app.wait(lambda: app.find(text='Select photos in your library, then choose Image > Add to “Café !🐙”.', kind='Label'))
        path = next((state/'apps/os.photos').rglob('library.json'))
        names = [entry['title'] for entry in json.loads(path.read_text())['albums']]
        assert 'Café !🐙' in names and 'Temporary' not in names, names

        # Leaving a newly opened name for Search must keep later typing there.
        key(app, 'KeyN', cmd=1)
        naming(app)
        search = app.wait(lambda: app.find(identifier='search', kind='TextInput'))
        app.click_row(search)
        time.sleep(.2)
        app.call('t', t='beach', wait=1)
        app.wait(lambda: (app.find(identifier='search', kind='TextInput') or {}).get('val') == 'beach')
        names = [entry['title'] for entry in json.loads(path.read_text())['albums']]
        assert 'Untitled Album' in names and 'beach' not in names, names
        app.check_logs()
    except Exception:
        app.capture('failed-state')
        raise
    finally:
        app.close()

    app = start('photos-restarted')
    try:
        album(app, 'Café !🐙')
        app.check_logs()
    finally:
        app.close()
    (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
        'checks': ['Typing replaces the selected album name',
                   'Arrow keys edit the original name at a Unicode character boundary',
                   'Escape restores the previous name',
                   'Moving to Search does not rename the album',
                   'The restored name survives restart']}, indent=2) + '\n')
    print('PASS: Photos album naming, focus, Escape and restart')


if __name__ == '__main__':
    main()
