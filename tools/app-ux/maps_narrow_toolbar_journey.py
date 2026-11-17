#!/usr/bin/env python3
"""Check Maps' narrow toolbar with disposable state on a desktop host."""
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
    parser.add_argument('--offline', action='store_true', help='Also check the map error notice; run with external network blocked')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-maps'], out, 'maps-narrow-toolbar')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def ident(name, kind=None):
        return native.wait(lambda: native.find(identifier=name, kind=kind), timeout=30)

    def resize(width):
        for _ in range(3):
            x, y, w, h = ident(None, 'MpModuleView')['r']
            if abs(w - width) < 2:
                return
            start, end = x + w - 2, x + width - 2
            native.call('m', k='move', x=start, y=y+h/2, wait=1)
            native.call('m', k='down', x=start, y=y+h/2, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=start + (end-start)*step/6, y=y+h/2, wait=1)
            native.call('m', k='up', x=end, y=y+h/2, wait=1)
            time.sleep(.6)
        assert abs(ident(None, 'MpModuleView')['r'][2] - width) < 12

    def fits(child, parent):
        x, y, w, h = child['r']
        a, b, c, d = parent['r']
        return x >= a and y >= b and x+w <= a+c+1 and y+h <= b+d+1

    def overlaps(first, second):
        x, y, w, h = first['r']
        a, b, c, d = second['r']
        return x < a+c and a < x+w and y < b+d and b < y+h

    def offline_notice():
        row("The map couldn't load. Check your connection.", 'Label')
        notice, tilemap = ident('map_load'), ident('map_keys', 'GestureView')
        assert fits(notice, tilemap) and fits(ident('map_load_again', 'Button'), notice)
        assert not overlaps(notice, ident('map_scale')) and not overlaps(notice, ident('map_zoom'))

    def other_appearance():
        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        time.sleep(1.5)  # Let the new view finish its appearance transition.

    try:
        ident('search', 'TextInput')
        row('No Places Yet', 'Label')
        checks.append('A fresh readable profile shows the empty state')
        time.sleep(.8)
        resize(480)
        if not native.find(identifier='search', kind='TextInput'):
            native.call('event', data='mac-row:mac.app.sidebar')
        ident('search', 'TextInput')
        bar, more = ident('map_bar'), ident('more_tool', 'View')
        assert all(fits(item, bar) for item in (more, row('2D'), row('3D')))
        checks.append('The compact toolbar and map mode buttons fit beside the sidebar')
        if args.offline:
            offline_notice()
            checks.append('The offline notice fits and avoids controls at minimum width')
        native.capture('compact-toolbar')

        native.click_row(more)
        for title in ('Standard', 'Light', 'Dark', 'Copy Link', 'Open in Browser'):
            row(title, 'PopupMenuItem')
        checks.append('The overflow menu exposes styles and sharing')
        native.click_row(row('Dark', 'PopupMenuItem'))
        native.call('event', data='mac-row:mac.app.sidebar')
        assert row('Dark') and fits(ident('style_box'), ident('map_bar'))
        assert fits(ident('share_tool'), ident('map_bar')) and not native.find(identifier='more_tool', kind=None)
        checks.append('A chosen style stays selected when the sidebar closes')

        native.call('event', data='mac-row:mac.app.sidebar')
        ident('more_tool', 'View')
        other_appearance()
        native.click_row(ident('more_tool', 'View'))
        time.sleep(.4)  # The popup's opening frame must finish before the press.
        native.capture('restyled-overflow')
        standard = row('Standard', 'PopupMenuItem')
        x, y, width, height = standard['r']
        with Image.open(out/'restyled-overflow.png') as picture:
            scale = picture.width / native.call('s')['w'][0]['sz'][0]
            rgb = picture.convert('RGB').getpixel((round((x + width - 12) * scale),
                                                  round((y + height / 2) * scale)))
        assert (sum(rgb) / 3 < 128) == (args.appearance == 'light'), rgb
        checks.append('The menu follows the new light or dark appearance')
        native.click_row(standard)
        time.sleep(.4)
        native.capture('restyled-overflow-after-select')
        resize(700)
        assert row('Standard') and not native.find(identifier='more_tool', kind=None)
        checks.append('Appearance and window changes restore the full toolbar')

        if args.offline:
            offline_notice()
            checks.append('The offline notice and retry fit after resizing')
            native.capture('offline-notice')

        native.check_logs()
        assert not any(marker in native.log_path.read_text() for marker in
                       ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'disposable desktop state', 'offline': args.offline}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
