#!/usr/bin/env python3
"""Exercise Writer's retry after a disposable conversion source write fails."""
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
    parser.add_argument('--action', choices=['save', 'preview', 'export'], required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out/'state'
    storage = state/'apps/os.writer'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    native = AppNative(args.binary.resolve(), ['--test-action', 'style:' + style,
        '--test-action', 'launch-writer'], out, 'writer-conversion-storage')
    blocked = None
    draft = 'A disposable Writer conversion.'
    checks = []

    def row(identifier=None, text=None, kind='Button'):
        return native.wait(lambda: native.find(identifier=identifier, text=text, kind=kind), timeout=30)

    def document():
        path = storage/'index.json'
        if not path.exists():
            return None
        try:
            docs = json.loads(path.read_text()).get('docs', [])
            return docs[0] if len(docs) == 1 else None
        except (ValueError, OSError):
            return None

    def editor_text():
        return (native.find(identifier='editor_text', kind='DocumentEditor') or {}).get('t')

    try:
        native.wait(lambda: native.find(text='No Documents Yet', kind='Label'), timeout=60)
        native.click_row(row(identifier='empty_new'))
        row(identifier='editor_text', kind='DocumentEditor')
        native.call('t', t=draft, wait=1)
        native.wait(lambda: editor_text() == draft)
        doc = native.wait(document, timeout=30)
        blocked = storage/'work'/(doc['id'] + '.md')
        blocked.parent.mkdir(parents=True, exist_ok=True)
        blocked.write_text('disposable fixture')
        blocked.chmod(0o444)

        if args.action == 'save':
            native.click_row(row(identifier='save_button'))
            words = "Writer couldn't save Word documents. Try again."
        elif args.action == 'preview':
            native.click_row(row(text='Preview'))
            words = "Writer couldn't show a preview. Try again."
        else:
            native.click_row(row(identifier='export_button'))
            words = "Writer couldn't export copies. Try again."

        native.label(words)
        row(identifier='editor_retry')
        assert editor_text() == draft, 'A failed conversion changed the draft'
        status = row(identifier='editor_status', kind='Label').get('t', '')
        assert 'Saving' not in status, 'The saving indicator did not finish'
        native.capture('conversion-write-failed')
        checks.append('A failed temporary write shows a retry and ends Saving')
        native.click_row(row(identifier='editor_retry'))
        time.sleep(.3)
        native.label(words)
        row(identifier='editor_retry')
        assert editor_text() == draft, 'A failed retry changed the draft'
        checks.append('A repeated failure leaves the draft and retry available')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        def restyled():
            log = native.log_path.read_text()[start:]
            at = log.find('desktop style ' + target + ' applied to ')
            return at >= 0 and '[SPLASH] eval:' in log[at:]
        native.wait(restyled, timeout=30)
        native.label(words)
        row(identifier='editor_retry')
        assert editor_text() == draft, 'An appearance switch changed the draft'
        checks.append('The failure and retry survive an appearance switch')

        blocked.chmod(0o644)
        native.click_row(row(identifier='editor_retry'))
        if args.action == 'preview':
            row(identifier='preview_scroll', kind=None)
        elif args.action == 'export':
            row(text='Cancel')
            row(identifier='export_doc', kind='Label')
        else:
            native.wait(lambda: not native.find(identifier='editor_retry', kind='Button'), timeout=60)

        def word_file():
            saved = document()
            path = storage/saved['docx'] if saved and saved.get('docx') else None
            return path if path and path.is_file() and path.read_bytes().startswith(b'PK') else None
        native.wait(word_file, timeout=60)
        native.capture('conversion-recovered')
        checks.append('Retry creates a Word file with the real local service')
        native.check_logs()
        log = native.log_path.read_text()
        assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit')), \
            'Script error or execution limit; inspect ' + native.log_path.name
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance, 'action': args.action,
            'checks': checks}, indent=2) + '\n')
        print('PASS:', args.appearance, args.action, len(checks), 'checks')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        if blocked is not None and blocked.exists():
            blocked.chmod(0o644)
        native.close()


if __name__ == '__main__':
    main()
