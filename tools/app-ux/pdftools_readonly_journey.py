#!/usr/bin/env python3
"""Check read-only PDF field navigation using a disposable local form."""
import argparse
import json
import os
from pathlib import Path
import time

from capture import AppNative, ROOT


def form_pdf():
    """A real two-page PDF with one editable and one read-only text field."""
    first = 'BT /Helv 12 Tf 20 270 Td (Editable page) Tj ET'
    second = 'BT /Helv 12 Tf 20 270 Td (Read only page) Tj ET'
    stream = lambda text: f'<< /Length {len(text)} >>\nstream\n{text}\nendstream'
    objects = [
        '<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 7 0 R] /NeedAppearances true /DA (/Helv 10 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >> >>',
        '<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>',
        '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /Helv 8 0 R >> >> /Annots [4 0 R] /Contents 5 0 R >>',
        '<< /Type /Annot /Subtype /Widget /FT /Tx /T (Editable) /V (before) /Rect [20 160 220 180] /P 3 0 R /F 4 /DA (/Helv 10 Tf 0 g) >>',
        stream(first),
        '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /Helv 8 0 R >> >> /Annots [7 0 R] /Contents 9 0 R >>',
        '<< /Type /Annot /Subtype /Widget /FT /Tx /T (Read-only reference) /V (fixed second page) /Rect [20 60 220 80] /P 6 0 R /F 4 /Ff 1 /DA (/Helv 10 Tf 0 g) >>',
        '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>',
        stream(second),
    ]
    pdf = bytearray(b'%PDF-1.4\n')
    offsets = []
    for number, body in enumerate(objects, 1):
        offsets.append(len(pdf))
        pdf.extend(f'{number} 0 obj\n{body}\nendobj\n'.encode())
    xref = len(pdf)
    pdf.extend(f'xref\n0 {len(objects) + 1}\n0000000000 65535 f \n'.encode())
    for offset in offsets:
        pdf.extend(f'{offset:010} 00000 n \n'.encode())
    pdf.extend(f'trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF'.encode())
    return bytes(pdf)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--appearance', choices=['light', 'dark'], default='light')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    library = state/'apps/os.pdftools/accounts/device/library'
    library.mkdir(parents=True)
    (library/'Reference form.pdf').write_bytes(form_pdf())
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-pdftools'], out, 'pdftools-readonly')
    checks = []

    def row(text, kind='Button'):
        return native.wait(lambda: native.find(text=text, kind=kind), timeout=60)

    def key(code, **modifiers):
        native.call('k', k='press', c=code, wait=1, **modifiers)

    def resize(width):
        for _ in range(3):
            x, y, w, h = native.wait(lambda: native.find(kind='MpModuleView'), timeout=30)['r']
            if abs(w - width) < 2:
                return
            start = x + w - 2
            end = x + width - 2
            native.call('m', k='move', x=start, y=y+h/2, wait=1)
            native.call('m', k='down', x=start, y=y+h/2, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=start + (end-start)*step/6, y=y+h/2, wait=1)
            native.call('m', k='up', x=end, y=y+h/2, wait=1)
            time.sleep(.6)
        native.wait(lambda: abs(native.find(kind='MpModuleView')['r'][2] - width) < 12, timeout=30)

    try:
        row('Reference form.pdf', 'Label')
        time.sleep(.8)  # The window's opening transition must finish before pointer input.
        native.click_row(row('Reference form.pdf', 'Label'))
        row('1 / 2', 'Label')
        native.click_row(row('Fill & Sign'))
        row('Read-only reference', 'Label')
        native.wait(lambda: any('fixed second page' in (r.get('t') or '') and 'read-only' in r['t']
            for r in native.rows() if r.get('ty') == 'Label'), timeout=30)
        checks.append('The real PDF service reports the field as read-only')
        native.click_row(row('Editable', 'Label'))
        row('before', 'TextInput')
        key('KeyA', cmd=1)
        native.call('t', t='after', wait=1)
        row('Edited', 'Label')
        native.wait(lambda: any(r.get('ty') == 'Label' and r.get('t') == '•' for r in native.rows()), timeout=30)
        key('KeyS', cmd=1)
        row('Saved', 'Label')
        native.wait(lambda: not any(r.get('ty') == 'Label' and r.get('t') == '•' for r in native.rows()), timeout=30)
        native.click_row(row('Editable', 'Label'))
        row('after', 'TextInput')
        checks.append('An active edit is marked Edited and saved with Command-S')
        native.click_row(row('after', 'TextInput'))
        key('KeyA', cmd=1)
        native.call('t', t='after again', wait=1)
        row('after again', 'TextInput')
        native.click_row(row('Read-only reference', 'Label'))
        row('2 / 2', 'Label')
        row('after again', 'Label')
        assert not native.find(identifier='field_input', kind='TextInput')
        checks.append('The field opens on page two after applying the previous edit')

        name = row('Read-only reference', 'Label')
        value = next(r for r in native.rows() if r.get('ty') == 'Label'
                     and 'fixed second page' in (r.get('t') or '') and 'read-only' in r['t'])
        heading = row('Value', 'Label')
        assert value['r'][0] - (name['r'][0] + name['r'][2]) >= 7
        assert abs(value['r'][0] - heading['r'][0]) <= 1
        native.capture('readonly-page')
        checks.append('Field names and values are separated and aligned')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        row('2 / 2', 'Label')
        key('ArrowLeft')
        row('1 / 2', 'Label')
        x, y, w, h = row('Read-only reference', 'Label')['r']
        native.call('m', k='move', x=x+w/2, y=y+h/2, wait=1)
        native.call('m', k='down', x=x+w/2, y=y+h/2, b=1, wait=1)
        native.call('m', k='up', x=x+w/2, y=y+h/2, b=1, wait=1)
        native.click_row(row('Go to Field', 'PopupMenuItem'))
        row('2 / 2', 'Label')
        checks.append('Restyle and the field context menu retain navigation')

        resize(700)
        def stacked():
            rows = native.rows()
            n = next((r for r in rows if r.get('ty') == 'Label' and r.get('t') == 'Read-only reference'), None)
            v = next((r for r in rows if r.get('ty') == 'Label' and 'fixed second page' in (r.get('t') or '')
                      and 'read-only' in r['t']), None)
            return n and v and abs(n['r'][0] - v['r'][0]) <= 1 and v['r'][1] > n['r'][1]
        native.wait(stacked, timeout=30)
        native.capture('narrow-readonly')
        checks.append('At 700 points the name and value stack vertically')

        key('KeyW', cmd=1)
        native.wait(lambda: any(r.get('ty') == 'Label' and 'Save the changes' in (r.get('t') or '')
                                for r in native.rows()), timeout=30)
        native.click_row(row('Save'))
        row('Recent', 'Label')
        time.sleep(.8)  # Let the close sheet release its pointer shield.
        native.click_row(row('Reference form.pdf', 'Label'))
        native.wait(lambda: any(r.get('ty') == 'Label' and r.get('t') in ('1 / 2', '2 / 2')
                                for r in native.rows()), timeout=30)
        native.call('event', data='mac-row:mac.app.mode_fill')
        native.click_row(row('Editable', 'Label'))
        row('after again', 'TextInput')
        checks.append('Saving on close retains the next edit when the PDF is reopened')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit'))
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'checks': checks, 'fixture': 'generated two-page local PDF'}, indent=2) + '\n')
        print('PASS:', args.appearance, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
