#!/usr/bin/env python3
"""Check macOS Spotlight selection, its preview and constrained windows."""
import argparse
import json
import os
from pathlib import Path
import time

from PIL import Image
from capture import AppNative, ROOT


def key(app, code):
    app.call('k', k='press', c=code, wait=1)
    time.sleep(.15)


def type_keys(app, text):
    for char in text:
        key(app, 'Key' + char.upper())
    time.sleep(.4)


def menu(app):
    return next((row for row in app.rows() if row.get('i') == 'shell_menu'), None)


def blue(rgb):
    r, g, b = rgb
    return b >= 150 and b > g * 1.35 and b > r * 1.6


def image_and_geometry(out, name):
    rows = json.loads((out / (name + '.snapshot.json')).read_text())
    card = next(row['r'] for row in rows if row.get('i') == 'shell_menu')
    window = next(row['r'] for row in rows if row.get('ty') == 'Window')
    image = Image.open(out / (name + '.png')).convert('RGB')
    return image, card, image.width / window[2]


def preview_icon(out, name):
    image, card, scale = image_and_geometry(out, name)
    left = int(card[2] * .52)
    x = card[0] + left + (card[2] - left - 64) / 2
    y = card[1] + 52 + 20
    crop = image.crop((round(x * scale), round(y * scale),
                       round((x + 64) * scale), round((y + 64) * scale)))
    pixels = list(crop.getdata())
    green = sum(1 for r, g, b in pixels if g > r * 1.3 and g >= b * .85
                and max(r, g, b) - min(r, g, b) > 60) / len(pixels)
    return pixels, green


def same_fraction(first, second, tolerance):
    assert len(first) == len(second)
    return sum(max(abs(a - b) for a, b in zip(x, y)) <= tolerance
               for x, y in zip(first, second)) / len(first)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'

    def start(name, size=None):
        state = out / ('state-' + name)
        os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
            OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
            MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file', OCTOSENSE_MAIL_VAULT='file',
            MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
        actions = ['--test-action', 'style:' + style]
        if size:
            actions += ['--test-action', 'window-size:' + size]
        actions += ['--test-action', 'menu']
        app = AppNative(args.binary.resolve(), actions, out, name)
        app.wait(lambda: menu(app) and menu(app).get('t', '').startswith('spotlight []'), timeout=90)
        return app

    app = start('wide')
    try:
        type_keys(app, 'ma')
        text = menu(app).get('t', '')
        assert 'Maps=Application' in text and 'Mail=Application' in text, text
        app.capture('maps')
        key(app, 'ArrowDown')
        app.capture('mail')
        key(app, 'ArrowUp')
        app.capture('maps-again')
        card = menu(app)['r']
        # Mail follows the Top Hit and Applications captions.
        app.call('m', k='move', x=card[0] + 80, y=card[1] + 160, wait=1)
        time.sleep(.4)
        app.capture('mail-hover')
        maps, mg = preview_icon(out, 'maps')
        mail, gg = preview_icon(out, 'mail')
        again, ag = preview_icon(out, 'maps-again')
        hover, hg = preview_icon(out, 'mail-hover')
        assert min(mg, gg, ag, hg) >= .03, 'the selected app icon is missing from the preview'
        assert same_fraction(maps, mail, 25) <= .92, 'the preview did not change with the selection'
        assert same_fraction(maps, again, 8) >= .90, 'the preview did not return to Maps'
        assert same_fraction(mail, hover, 8) >= .90, 'the preview did not follow the pointer'
        type_keys(app, 'zzq')
        assert '=' not in menu(app).get('t', ''), 'a nonsense search returned results'
        app.capture('no-results')
        key(app, 'Escape')
        type_keys(app, 'dark')
        assert 'Appearance=System Settings' in menu(app).get('t', '')
        app.capture('settings-result')
        app.check_logs()
    finally:
        app.close()

    app = start('scroll', '700x460')
    try:
        type_keys(app, 'a')
        assert menu(app).get('t', '').count('=') >= 7
        for _ in range(6):
            key(app, 'ArrowDown')
        app.capture('scrolled')
        image, card, scale = image_and_geometry(out, 'scrolled')
        region = image.crop((round((card[0] + 8) * scale), round((card[1] + 58) * scale),
                             round((card[0] + card[2] * .51) * scale), round((card[1] + card[3] - 6) * scale)))
        assert sum(blue(pixel) for pixel in region.getdata()) / (region.width * region.height) > .025, 'selected result scrolled out of view'
        app.check_logs()
    finally:
        app.close()

    app = start('narrow', '480x560')
    try:
        type_keys(app, 'ma')
        app.capture('narrow-results')
        image, card, scale = image_and_geometry(out, 'narrow-results')
        sample = image.getpixel((round((card[0] + card[2] * .75) * scale), round((card[1] + 95) * scale)))
        assert blue(sample), 'the narrow result did not retain its full-width selection'
        key(app, 'Escape')
        type_keys(app, 'calc')
        assert 'Calculator=Application' in menu(app).get('t', '')
        key(app, 'ReturnKey')
        app.wait(lambda: any(row.get('ty') == 'MpModuleView' for row in app.rows()), timeout=60)
        app.check_logs()
    finally:
        app.close()

    (out / 'receipt.json').write_text(json.dumps({'appearance': args.appearance,
        'checks': ['The selected preview follows keyboard and pointer',
                   'No results and a System Settings result remain readable',
                   'A long result list follows the keyboard selection',
                   'A narrow window keeps a full-width list and can open an app']}, indent=2) + '\n')
    print('PASS: Spotlight preview, search, scrolling and narrow-window navigation')


if __name__ == '__main__':
    main()
