#!/usr/bin/env python3
"""Read-only feed browsing plus app-local saved-story persistence."""
import argparse
import json
import os
from pathlib import Path
import time
from capture import AppNative, ROOT


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    os.environ.update(OCTOSENSE_HOME=str(out/'home'), OCTOSENSE_APP_DATA=str(out/'apps'), OCTOS_APP_CORE_DIR=str(out/'core'))
    title = None
    for attempt in range(2):
        a = AppNative(args.binary.resolve(), ['--test-action', 'phone:ios', '--test-action', 'launch-news'], out, 'news-'+str(attempt))
        try:
            a.label('News')
            time.sleep(1)
            if attempt == 0:
                row = a.wait(lambda: next((r for r in a.rows() if r.get('ty') == 'Label' and r.get('i') == 'title' and r.get('t')), None), timeout=40)
                title = row['t']
                a.capture('news-feed')
                a.click_row(row)
                a.click('Save')
                a.wait(lambda: a.find(text='Saved'))
                a.click('‹ News')
                a.click('Saved')
                a.label(title)
                a.capture('news-saved')
                a.field('search', 'no-match-this-native-test')
                a.call('k', k='press', c='Escape', wait=1)
                time.sleep(.3)
                assert not any(r.get('i') == 'title' and r.get('ty') == 'Label' for r in a.rows())
                a.label('No stories match your search.')
                a.capture('news-empty-search')
            else:
                a.click('Saved')
                a.label(title)
                a.capture('news-restored')
            a.check_logs()
        except Exception:
            a.capture('failed-state')
            raise
        finally:
            a.close()
    (out/'receipt.json').write_text(json.dumps({'checks': ['Live feed data', 'Open reader and save locally', 'Saved filter and empty search', 'Saved story retained after process restart'], 'physical_mobile': False, 'saved_title': title}, indent=2)+'\n')
    print('PASS: feed, reader, saved state, search and restart')


if __name__ == '__main__':
    main()
