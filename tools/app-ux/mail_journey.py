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
    p.add_argument('--desktop', action='store_true', help='The desktop tree: mailboxes, list and message side by side')
    p.add_argument('--appearance', choices=['light', 'dark'], help='Choose the Mac desktop appearance')
    args = p.parse_args()
    if args.appearance and not args.desktop:
        p.error('--appearance requires --desktop')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / 'state'
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
        OCTOS_APP_CORE_DIR=str(state/'core'), OCTOSENSE_LLM_VAULT='file', MAKEPAD_APP_CONFIG='{"mail_demo":true}')
    actions = ['--test-action', 'launch-mail'] if args.desktop else ['--test-action', 'phone:ios', '--test-action', 'launch-mail']
    if args.appearance:
        actions = ['--test-action', 'style:macos-dark' if args.appearance == 'dark' else 'style:macos'] + actions
    native = AppNative(args.binary.resolve(), actions, out, 'mail')
    def field(key, text):
        if key == 'password':
            native.click_row(native.reachable(identifier=key, kind='TextInput'))
            native.call('t', t=text, wait=1)
        else:
            native.field(key, text)
        # Escape puts a phone's keyboard away; on a Mac it cancels the sheet, as it should.
        if not args.desktop:
            native.call('k', k='press', c='Escape', wait=1)
        time.sleep(.3)
    try:
        if args.desktop:
            native.label('No Mail Account')
            time.sleep(.8)
            native.click('Add Account…')
            native.label('Add a Mail Account')
        else:
            native.label('Your inbox, with room to focus.')
            time.sleep(.8)
            native.click('Add account')
            native.label('OctoSense · Add a mail account')
        field('address', 'ux-test@example.com')
        field('password', 'demo')
        native.click('Sign In' if args.desktop else 'Sign in')
        native.label('Dinner on Saturday?')
        native.capture('mail-inbox')
        native.click_row(native.reachable(text='Dinner on Saturday?', kind='Label'))
        native.label('We are thinking of trying the new place')
        native.capture('mail-reader')
        if args.desktop:
            # Select another row and immediately use Forward. The old message
            # must never be quoted while the new one is still loading.
            native.click_row(native.reachable(text='Welcome to Mail', kind='Label'))
            native.click_row(native.wait(lambda: native.find(identifier='forward_on', kind='Button')))
            native.wait(lambda: (native.find(identifier='c_subject', kind='TextInput') or {}).get('t') == 'Fwd: Welcome to Mail')
            native.capture('mail-forward-after-selection')
            native.click_row(native.wait(lambda: native.find(identifier='compose_close', kind='Button')))
            native.wait(lambda: not native.find(identifier='c_body', kind='TextInput'))
            native.click_row(native.reachable(text='Dinner on Saturday?', kind='Label'))
            native.label('We are thinking of trying the new place')
            # A toolbar's glyph has no words: its name is its identifier.
            native.click_row(native.wait(lambda: native.find(identifier='reply_on', kind='Button')))
        else:
            native.click('Reply')
        native.wait(lambda: native.find(identifier='c_body', kind='TextInput'))
        field('c_body', 'UX test draft only.\n第二行：检查键盘和文字选择。')
        native.capture('mail-draft')
        if args.desktop:
            # The New Message window's close button: its red light on a Mac, 12 points, so it is
            # pressed where it is drawn; `reachable` asks for a 28-point row, a finger's size.
            native.click_row(native.wait(lambda: native.find(identifier='compose_close', kind='Button')))
            # What was written is not lost by a stray click; the message stays selected.
            native.label('Discard this message?')
            native.label("Your changes won't be kept.")
            native.click('Discard')
            native.wait(lambda: not native.find(identifier='c_body', kind='TextInput'))
            native.label('We are thinking of trying the new place')
            native.label('Mailboxes')
        else:
            native.click('Cancel')
            native.label('Discard this message?')
            native.label("Your changes won't be kept.")
            native.click('Keep Editing')
            native.wait(lambda: (native.find(identifier='c_body', kind='TextInput') or {}).get('t') == 'UX test draft only.\n第二行：检查键盘和文字选择。')
            native.click('Cancel')
            native.click('Discard Message')
            native.label('Inbox')
            native.click('Folders')
            native.label('Folders')
        native.capture('mail-folders')
        native.check_logs()
        checks = ['Host-owned account sheet', 'Inbox data and reader', 'Reply with multiline Unicode input', 'Cancel and folders']
        if args.desktop:
            checks.append('Forward after switching the selected message')
        (out/'receipt.json').write_text(json.dumps({'physical_mobile': False, 'transport': 'local demo',
            'checks': checks,
            'sent_messages': 0, 'actions': native.actions}, indent=2)+'\n')
        print('PASS: local demo Mail account, inbox, read, reply draft, cancel, folders; nothing sent')
    except Exception:
        native.capture('failed-state')
        raise
    finally:
        native.close()


if __name__ == '__main__':
    main()
