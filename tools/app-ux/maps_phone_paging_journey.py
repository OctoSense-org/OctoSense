#!/usr/bin/env python3
"""Check Maps' phone-sized saved list and the layers above its map."""
import argparse
import json
import os
from pathlib import Path
import time

from PIL import Image

from capture import AppNative, ROOT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    parser.add_argument('--source', type=Path, default=ROOT/'apps/maps/bundle/main.splash',
                        help='Expected packaged script')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    data = state/'apps/os.maps/accounts/device'
    data.mkdir(parents=True)
    saved = data/'saved.json'
    saved.write_text(json.dumps([{'name': f'Synthetic Place {i:03d}', 'cat': 'Cafe',
        'label': 'Sample City', 'lat': 37.335 + i*.0001, 'lon': -121.885} for i in range(500)]))
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'ios-dark' if args.appearance == 'dark' else 'ios'
    native = None
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def open_saved():
        # The field can appear before the last Saved batch has been read.
        for attempt in range(4):
            native.click_row(ident('search', 'TextInput'))
            try:
                return native.wait(lambda: native.find(text='Synthetic Place 000', kind='Label'), timeout=6)
            except AssertionError:
                if attempt == 3:
                    raise
                time.sleep(.5)

    try:
        native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
            '--test-action', 'launch-maps'], out, 'maps-phone-paging')
        # The saved list loads in background batches after the search field appears.
        time.sleep(3)
        native.click_row(open_saved())
        row('Directions')
        assert ident('pname', 'Label').get('t') == 'Synthetic Place 000'
        checks.append('A saved place opens in the phone-sized preview')
        scripts = list((state/'apps/.system/os.maps').glob('*/main.splash'))
        assert scripts and all(p.read_bytes() == args.source.read_bytes() for p in scripts)
        checks.append('The packaged Maps script matches the source under test')
        time.sleep(1)  # Let the keyboard and place-card transition settle.
        native.click_row(row('Saved'))
        row('Save')
        assert len(json.loads(saved.read_text())) == 499
        native.click_row(row('Save'))
        row('Saved')
        assert len(json.loads(saved.read_text())) == 500
        checks.append('Removing and saving the place preserves the other places')
        native.click_row(row('Close'))
        native.click_row(ident('search', 'TextInput'))
        native.click_row(row('Next'))
        row('101–200 of 500', 'Label')
        checks.append('The next page opens')

        start = len(native.log_path.read_text())
        width, height = native.call('s')['w'][0]['sz']
        native.call('click', x=width-70, y=14, wait=1)
        native.wait(lambda: '[SPLASH] eval:' in native.log_path.read_text()[start:], timeout=30)
        time.sleep(1)
        native.click_row(ident('search', 'TextInput'))
        row('101–200 of 500', 'Label')
        row('Synthetic Place 101', 'Label')
        checks.append('The page survives an appearance change')
        time.sleep(1)  # Capture the settled keyboard, not an animation frame.
        native.capture('restyled-saved-page')

        # The fixture's sheet contains no orange content. Map pins that escape
        # their layer are therefore visible as orange pixels inside its bounds.
        x, y, w, h = ident('search_panel')['r']
        with Image.open(out/'restyled-saved-page.png') as image:
            sx, sy = image.width / width, image.height / height
            area = image.convert('RGB').crop((int((x+12)*sx), int((y+12)*sy),
                int((x+w-12)*sx), int(min(y+h-12, height*.62)*sy)))
            orange = sum(1 for r, g, b in area.getdata()
                         if r > 180 and 65 < g < 190 and b < 90 and r > g*1.2)
        assert orange < 8, f'{orange} orange map pixels cover the search sheet'
        checks.append('Map pins stay behind the search sheet')
        native.click_row(row('Synthetic Place 101', 'Label'))
        row('Directions')
        assert ident('pname', 'Label').get('t') == 'Synthetic Place 101'
        checks.append('A place on the retained page opens')
        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'checks': checks,
            'orange_pixels_in_sheet': orange, 'fixture': '500 disposable places in a phone-sized Mac preview'},
            indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks; orange pixels:', orange)
    except Exception:
        if native:
            native.capture('failed-state')
        raise
    finally:
        if native:
            native.close()


if __name__ == '__main__':
    main()
