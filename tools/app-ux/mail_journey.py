#!/usr/bin/env python3
"""Test Mail using its explicit local demo transport, without sending mail."""
import argparse
import json
import os
from pathlib import Path
import time
from capture import AppNative, ROOT


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / 'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), OCTOSENSE_LLM_VAULT='file', MAKEPAD_APP_CONFIG='{"mail_demo":true}')
    native = AppNative(args.binary.resolve(), ['--test-action', 'phone:ios', '--test-action', 'launch-mail'], out, 'mail')
    def field(key, text):
        if key == 'password':
            native.click_row(native.reachable(identifier=key, kind='TextInput'))
            native.call('t', t=text, wait=1)
        else:
            native.field(key, text)
        native.call('k', k='press', c='Escape', wait=1)
        time.sleep(.3)
    try:
        native.label('Your inbox, with room to focus.')
        time.sleep(.8)
        native.click('Add account')
        native.label('OctoSense · Add a mail account')
        field('address', 'ux-test@example.com')
        field('password', 'demo')
        native.click('Sign in')
        native.label('Dinner on Saturday?')
        native.capture('mail-inbox')
        native.click_row(native.reachable(text='Dinner on Saturday?', kind='Label'))
        native.label('We are thinking of trying the new place')
        native.capture('mail-reader')
        native.click('Reply')
        native.wait(lambda: native.find(identifier='c_body', kind='TextInput'))
        field('c_body', 'UX test draft only.\n第二行：检查键盘和文字选择。')
        native.capture('mail-draft')
        native.click('Cancel')
        native.label('Inbox')
        native.click('Folders')
        native.label('Folders')
        native.capture('mail-folders')
        native.check_logs()
        (out/'receipt.json').write_text(json.dumps({'physical_mobile': False, 'transport': 'local demo',
            'checks': ['Host-owned account sheet', 'Inbox data and reader', 'Reply with multiline Unicode input', 'Cancel and folders'],
            'sent_messages': 0, 'actions': native.actions}, indent=2)+'\n')
        print('PASS: local demo Mail account, inbox, read, reply draft, cancel, folders; nothing sent')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
