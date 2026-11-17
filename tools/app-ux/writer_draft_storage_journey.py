#!/usr/bin/env python3
"""Check Writer's draft recovery with a disposable, read-only storage file."""
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
    parser.add_argument('--scenario', choices=['draft', 'index', 'index-revert', 'new-index'], required=True)
    parser.add_argument('--narrow', action='store_true', help='also check the warning with the sidebar open at 700 points')
    args = parser.parse_args()

    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    state = out / 'state'
    storage = state / 'apps/os.writer'
    index = storage / 'index.json'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
        MAKEPAD_HOME=str(state/'makepad'), OCTOSENSE_LLM_VAULT='file',
        MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    style = 'macos-dark' if args.appearance == 'dark' else 'macos'
    actions = ['--test-action', 'style:' + style, '--test-action', 'launch-writer']
    original = 'A disposable Writer draft.'
    changed = original + ' New text.'
    blocked = None
    native = None
    checks = []

    if args.scenario == 'new-index':
        storage.mkdir(parents=True)
        index.write_text(json.dumps({'schema': 1, 'docs': []}))
        index.chmod(0o444)
        blocked = index

    try:
        native = AppNative(args.binary.resolve(), actions, out, 'writer-draft-storage')

        def row(identifier=None, text=None, kind='Button'):
            return native.wait(lambda: native.find(identifier=identifier, text=text, kind=kind), timeout=30)

        def text():
            editor = native.find(identifier='editor_text', kind='DocumentEditor')
            return None if editor is None else editor.get('t', '')

        def key(code, **modifiers):
            native.call('k', k='press', c=code, wait=1, **modifiers)

        def draft_file(expected=None):
            drafts = list((storage/'drafts').glob('*.txt'))
            if len(drafts) != 1:
                return None
            if expected is not None and drafts[0].read_text() != expected:
                return None
            return drafts[0]

        def check_logs():
            native.check_logs()
            log = native.log_path.read_text()
            assert not any(marker in log for marker in ('[E] splash:', 'time budget', 'instruction limit')), \
                'Script error or execution limit; inspect ' + native.log_path.name

        native.wait(lambda: native.find(text='No Documents Yet', kind='Label'), timeout=60)
        native.click_row(row(identifier='empty_new'))
        row(identifier='editor_text', kind='DocumentEditor')
        native.call('t', t=original, wait=1)
        native.wait(lambda: text() == original)

        if args.scenario == 'new-index':
            expected = original
        else:
            file = native.wait(lambda: draft_file(original), timeout=30)
            native.wait(lambda: index.exists() and len(json.loads(index.read_text()).get('docs', [])) == 1, timeout=30)
            blocked = file if args.scenario == 'draft' else index
            blocked.chmod(0o444)
            native.call('t', t=' New text.', wait=1)
            native.wait(lambda: text() == changed)
            expected = changed

        row(identifier='draft_retry')
        native.label("Your latest changes aren't saved.")
        native.capture('write-failed')
        checks.append('A failed draft or index write shows a retryable notice')

        if args.narrow:
            windows = [r for r in native.rows() if r.get('ty') in ('MpModuleView', 'MpRunView')]
            rect = max(windows, key=lambda r: r['r'][2] * r['r'][3])['r']
            x, y = rect[0] + rect[2] - 2, rect[1] + rect[3] * .5
            to = rect[0] + 700 - 2
            native.call('m', k='move', x=x, y=y, wait=1)
            native.call('m', k='down', x=x, y=y, wait=1)
            for step in range(1, 7):
                native.call('m', k='move', x=x + (to - x) * step / 6, y=y, wait=1)
            native.call('m', k='up', x=to, y=y, wait=1)
            time.sleep(1)
            native.click_row(row(identifier='sidebar_tool2'))
            row(text='All Documents')
            row(identifier='draft_retry')
            native.capture('write-failed-narrow-sidebar')
            checks.append('The warning and retry remain available in a narrow window with the sidebar')

        # Both navigation and File > New would replace the page. Neither may
        # discard its only in-memory copy while the write is still blocked.
        native.click_row(row(text='All Documents'))
        assert text() == expected, 'Navigating away lost the pending edits'
        key('KeyN', cmd=1)
        assert text() == expected, 'New Document replaced the pending edits'
        native.click_row(row(identifier='draft_retry'))
        row(identifier='draft_retry')
        assert text() == expected, 'A failed retry changed the page'
        checks.append('Navigation, New Document and a failed retry keep the page')

        target = 'macos' if args.appearance == 'dark' else 'macos-dark'
        start = len(native.log_path.read_text())
        native.call('event', data='mac-row:mac.appearance.flip')
        native.wait(lambda: ('desktop style ' + target + ' applied to ') in native.log_path.read_text()[start:], timeout=30)
        native.wait(lambda: text() == expected, timeout=30)
        row(identifier='draft_retry')
        checks.append('The warning and pending text survive an appearance switch')

        if args.scenario == 'index-revert':
            # A draft write may have succeeded before its index write failed.
            # Reverting to the previously kept text must still rewrite the file.
            native.click_row(row(identifier='editor_text', kind='DocumentEditor'))
            key('KeyA', cmd=1)
            native.call('t', t=original, wait=1)
            native.wait(lambda: text() == original)
            time.sleep(1)
            row(identifier='draft_retry')
            expected = original
            checks.append('Reverting text still requires a successful write')

        blocked.chmod(0o644)
        native.click_row(row(identifier='draft_retry'))
        native.wait(lambda: not native.find(identifier='draft_retry', kind='Button'), timeout=30)
        file = native.wait(lambda: draft_file(expected), timeout=30)
        native.wait(lambda: len(json.loads(index.read_text()).get('docs', [])) == 1, timeout=30)
        checks.append('Retry stores the current page and its index')
        native.click_row(row(text='All Documents'))
        page = native.wait(lambda: next((r for r in native.rows() if r.get('ty') == 'Label'
            and r.get('t') == expected and r['r'][1] > 200), None), timeout=30)
        native.click_row(page)
        native.wait(lambda: text() == expected, timeout=30)
        native.capture('recovered-draft')
        checks.append('The recovered draft opens intact')
        check_logs()
        native.close()
        native = None

        native = AppNative(args.binary.resolve(), actions, out, 'writer-draft-restart')
        native.wait(lambda: native.find(text='All Documents', kind='Button'), timeout=60)
        native.click_row(row(text='All Documents'))
        page = native.wait(lambda: next((r for r in native.rows() if r.get('ty') == 'Label'
            and r.get('t') == expected and r['r'][1] > 200), None), timeout=30)
        native.click_row(page)
        native.wait(lambda: text() == expected, timeout=30)
        checks.append('The recovered draft survives a restart')
        # File > New can be pressed before the normal autosave timer fires.
        # The old document must be flushed before the editor is replaced.
        native.click_row(row(identifier='editor_text', kind='DocumentEditor'))
        key('KeyA', cmd=1)
        latest = expected + ' Before new.'
        native.call('t', t=latest, wait=1)
        key('KeyN', cmd=1)
        native.wait(lambda: text() == '', timeout=30)
        native.wait(lambda: draft_file(latest), timeout=30)
        native.click_row(row(text='All Documents'))
        page = native.wait(lambda: next((r for r in native.rows() if r.get('ty') == 'Label'
            and r.get('t') == latest and r['r'][1] > 200), None), timeout=30)
        native.click_row(page)
        native.wait(lambda: text() == latest, timeout=30)
        checks.append('New Document first keeps a just-edited page')
        check_logs()
        (out/'receipt.json').write_text(json.dumps({'appearance': args.appearance,
            'scenario': args.scenario, 'checks': checks}, indent=2) + '\n')
        print('PASS:', args.appearance, args.scenario, len(checks), 'checks')
    except Exception:
        if native:
            native.capture('failed-state')
        raise
    finally:
        if blocked is not None and blocked.exists():
            blocked.chmod(0o644)
        if native:
            native.close()


if __name__ == '__main__':
    main()
