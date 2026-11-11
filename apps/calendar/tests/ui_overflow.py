#!/usr/bin/env python3
"""Check crowded Mac Calendar hours with a hidden release shell and disposable events."""
import argparse
from datetime import date, timedelta
import json
import os
from pathlib import Path
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/app-ux'))
from calendar_journey import CalendarNative  # noqa: E402
from capture import AppNative  # noqa: E402


def event(ident, title, start, end):
    return {'id': ident, 'title': title, 'start': start, 'end': end, 'location': '', 'notes': ''}


def stamp(day, hour):
    return f'{day.isoformat()}T{hour:02d}:00'


def label(native, text):
    return native.wait(lambda: next((row for row in native.rows()
                                    if row.get('ty') == 'Label' and row.get('t') == text), None), timeout=30)


def segment(native, name):
    native.click_row(native.wait(lambda: native.find(text=name, kind='Button'), timeout=30))
    time.sleep(.3)


def launch(binary, out, mode, name, events):
    state = out / f'{mode}-{name}-state'
    if state.exists():
        raise RuntimeError(f'Choose a fresh output directory: {state}')
    store = state / 'apps/.host/calendar/events.json'
    store.parent.mkdir(parents=True)
    store.write_text(json.dumps(sorted(events, key=lambda item: (item['start'], item['id']))))
    env = dict(os.environ, OCTOSENSE_HOME=str(state / 'home'), OCTOS_APP_CORE_DIR=str(state / 'core'),
               OCTOSENSE_APP_DATA=str(state / 'apps'), OCTOSENSE_LLM_VAULT='file',
               MAKEPAD_SPLASH_BUDGET_MS='1000', MAKEPAD_WM_NO_PREBUILD='1', MAKEPAD_WM_NO_WARM='1')
    native = CalendarNative(binary, ['--test-action', f'style:macos{mode}',
                                     '--test-action', 'launch-calendar'], out, f'{mode or "light"}-{name}', env=env)
    try:
        native.wait(lambda: native.find(text='Calendars', kind='Label'), timeout=90)
        time.sleep(2)
    except Exception:
        native.close()
        raise
    return native


def narrow(native, width):
    x, y, w, h = native.wait(lambda: native.find(kind='MpModuleView'))['r']
    native.call('m', k='move', x=x + w, y=y + h, wait=1)
    native.call('m', k='down', x=x + w, y=y + h, wait=1)
    for fraction in (.3, .6, 1):
        native.call('m', k='move', x=x + w + (width - w) * fraction, y=y + h, wait=1)
    native.call('m', k='up', x=x + width, y=y + h, wait=1)
    native.wait(lambda: abs(native.find(kind='MpModuleView')['r'][2] - width) < 12, timeout=15)
    time.sleep(.5)


def crowded(binary, out, mode, today):
    events = [event(f'a{i}', f'Concurrent {i}', stamp(today, 9), stamp(today, 10)) for i in range(6)]
    events.append(event('b0', 'Long meeting', stamp(today, 11), stamp(today, 14)))
    events += [event(f'b{i}', f'Staggered {i}', stamp(today, 11 if i <= 2 else 12),
                     stamp(today, 12 if i <= 2 else 13)) for i in range(1, 5)]
    native = launch(binary, out, mode, 'crowded', events)
    try:
        segment(native, 'Week')
        count = label(native, '+5')
        label(native, 'Concurrent 0')
        twos = [row for row in native.rows() if row.get('ty') == 'Label' and row.get('t') == '+2']
        assert len(twos) == 2 and twos[0]['r'][1] != twos[1]['r'][1], twos
        native.capture(f'week{mode}')
        native.click_row(count)
        for i in range(6):
            label(native, f'Concurrent {i}')
        segment(native, 'Week')
        narrow(native, 700)
        count = label(native, '+5')
        label(native, 'Concurrent 0')
        native.capture(f'week-700{mode}')
        native.click_row(count)
        for i in range(6):
            label(native, f'Concurrent {i}')
        segment(native, 'Week')
        native.click_row(label(native, '+2'))
        for i in range(1, 5):
            label(native, f'Staggered {i}')
        native.check_logs()
    finally:
        native.close()


def overnight(binary, out, mode, today):
    events = [event('single', 'Morning event', stamp(today, 8), stamp(today, 9)),
              event('span', 'Overnight event', stamp(today, 11), stamp(today + timedelta(days=1), 1))]
    native = launch(binary, out, mode, 'overnight', events)
    try:
        segment(native, 'Week')
        for title in ('Morning event', 'Overnight event'):
            row = label(native, title)
            assert row['r'][2] >= 55, f'{title} lost its column: {row["r"]}'
        native.check_logs()
    finally:
        native.close()


def day_overflow(binary, out, mode, today):
    events = [event(f'c{i:02}', f'Crowd {i:02}', stamp(today, 9), stamp(today, 10)) for i in range(48)]
    native = launch(binary, out, mode, 'day', events)
    try:
        segment(native, 'Day')
        if native.find(text='Calendars', kind='Label'):
            native.call('k', k='press', c='KeyS', cmd=1, ctrl=1, wait=1)
            native.wait(lambda: not native.find(text='Calendars', kind='Label'), timeout=15)
        more = native.wait(lambda: next((row for row in native.rows() if row.get('ty') == 'Label'
                                         and (row.get('t') or '').startswith('+')
                                         and row['t'][1:].isdigit()), None), timeout=15)
        first_hidden = 48 - int(more['t'][1:])
        native.click_row(more)
        label(native, 'Calendars')
        row = label(native, f'Crowd {first_hidden:02}')
        module = native.find(kind='MpModuleView')['r']
        assert row['r'][0] < module[0] + 210, f'Hidden event is not in the sidebar: {row["r"]}'
        native.capture(f'day-overflow{mode}')
        x, y, width, height = row['r']
        for attempt in range(4):
            for _ in range(2):
                AppNative.call(native, 'click', x=x + width / 2, y=y + height / 2)
            try:
                editor = native.wait(lambda: native.find(identifier='e_title', kind='TextInput'), timeout=4)
                break
            except AssertionError:
                if attempt == 3:
                    raise
                time.sleep(.6)
        assert editor.get('t') == f'Crowd {first_hidden:02}', editor
        native.check_logs()
    finally:
        native.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/octosense')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    for mode in ('', '-dark'):
        for journey in (crowded, overnight, day_overflow):
            journey(args.binary.resolve(), out, mode, date.today())
            print(f'{mode or "light"}: {journey.__name__} passed', flush=True)


if __name__ == '__main__':
    main()
