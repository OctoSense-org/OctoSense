#!/usr/bin/env python3
"""Exercise empty-provider guidance and cancel the real host-owned add sheet."""
import argparse, json, os, time
from pathlib import Path
from capture import AppNative, ROOT

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args(); out=a.output.resolve(); out.mkdir(parents=True,exist_ok=True)
os.environ.update(OCTOSENSE_HOME=str(out/'home'),OCTOSENSE_APP_DATA=str(out/'apps'),
                  OCTOS_APP_CORE_DIR=str(out/'core'),RINX_DATA_DIR=str(out/'rinx'),OCTOSENSE_LLM_VAULT='file')
n=AppNative(ROOT/'target/release/octosense',['--test-action','phone:ios','--test-action','launch-ai-providers'],out,'providers')
try:
    n.label('Connect your first model'); time.sleep(.7)
    assert not n.find(text='Talk to Octos')
    assert not n.find(text='Show QR for phone')
    assert n.find(text='Add model')['r'][3] >= 44
    n.capture('empty')
    n.click('Add model'); n.wait(lambda:n.find(text='Cancel',identifier='-'))
    n.capture('host-add-sheet')
    n.click_row(n.find(text='Cancel',identifier='-')); n.label('Connect your first model')
    assert not n.find(text='Show QR for phone')
    n.capture('cancelled'); n.check_logs()
    (out/'receipt.json').write_text(json.dumps({'status':'passed','checks':[
        'Empty list has one reachable Add model primary action',
        'Unavailable chat and QR export are absent until a model exists',
        'Add model opens the real host-owned provider sheet',
        'Cancel returns without saving a provider'], 'physical_mobile':False,'provider_calls':False},indent=2)+'\n')
    print('PASS: empty guidance, host-owned add sheet, cancel without mutation')
except Exception:
    n.capture('failed-state'); raise
finally:n.close()
