#!/usr/bin/env python3
"""Expand the Calendar journey's real publication and retain an unsent chat draft."""
import argparse
import json
import os
from pathlib import Path
import time
from capture import AppNative, ROOT


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    p.add_argument('--calendar-state', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = args.calendar_state.resolve()
    if not (state/'apps/.host/calendar/events.json').is_file():
        p.error('Run calendar_journey.py first and use its isolated state directory')
    os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                      OCTOS_APP_CORE_DIR=str(state/'core'), OCTOSENSE_LLM_VAULT='file')
    a = AppNative(args.binary.resolve(), ['--test-action', 'launch-calendar', '--test-action', 'glance'], out, 'glance')
    draft = 'Keep this unsent draft · 未发送'
    try:
        a.wait(lambda: a.find(text='+ Event'), timeout=30)
        time.sleep(2)
        # Close/reopen using the keyboard so its first published card has focus.
        for _ in range(2):
            a.call('k', k='press', c='F9', wait=1)
            time.sleep(.5)
        a.capture('summary')
        a.call('k', k='press', c='ReturnKey', wait=1)
        a.wait(lambda: a.find(identifier='chat_tab'))
        assert 'Design review' in a.log_path.read_text() or 'os.calendar/' in a.log_path.read_text()
        a.capture('expanded')
        a.click('Chat')
        a.field('input', draft)
        a.capture('chat-draft')
        a.click('Card')
        a.click('Chat')
        assert a.find(identifier='input', kind='TextInput')['t'] == draft
        a.call('k', k='press', c='Escape', wait=1)
        time.sleep(.3)
        a.call('k', k='press', c='F9', wait=1)
        time.sleep(.3)
        a.call('k', k='press', c='ReturnKey', wait=1)
        a.wait(lambda: a.find(identifier='chat_tab'))
        a.click('Chat')
        assert a.find(identifier='input', kind='TextInput')['t'] == draft
        a.capture('chat-restored')
        a.check_logs()
        (out/'receipt.json').write_text(json.dumps({'checks': [
            'Open real Calendar publication using keyboard focus',
            'Expanded Card/Chat retains a Unicode unsent draft',
            'Same draft survives Card/Chat and collapse/reopen'],
            'model_calls': False, 'physical_mobile': False}, indent=2)+'\n')
        print('PASS: summary, expand, chat draft, card tab and collapse/reopen')
    except Exception:
        a.capture('failed-state')
        raise
    finally:
        a.close()


if __name__ == '__main__':
    main()
