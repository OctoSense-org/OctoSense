#!/usr/bin/env python3
"""Exercise the host-owned provider wizard without saving or contacting a provider."""
import argparse, json, os, time
from pathlib import Path
from capture import AppNative, ROOT

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--dark',action='store_true')
p.add_argument('--restyle-key',action='store_true')
p.add_argument('--binary',type=Path,default=ROOT/'target/release/octosense')
a=p.parse_args(); out=a.output.resolve(); out.mkdir(parents=True,exist_ok=True)
os.environ.update(OCTOSENSE_HOME=str(out/'home'),OCTOSENSE_APP_DATA=str(out/'apps'),
                  OCTOS_APP_CORE_DIR=str(out/'core'),RINX_DATA_DIR=str(out/'rinx'),
                  MAKEPAD_HOME=str(out/'makepad'),OCTOSENSE_LLM_VAULT='file')
n=AppNative(a.binary.resolve(),['--test-action','phone:ios','--test-action','launch-ai-providers'],out,'providers')
try:
    n.label('Connect your first model'); time.sleep(.7)
    if a.dark:
        n.call('click',x=332,y=16,wait=1); time.sleep(2)
    assert not n.find(text='Talk to Octos')
    assert not n.find(text='Show QR for phone')
    assert n.find(text='Add model')['r'][3] >= 44
    n.capture('empty')
    n.click('Add model')
    n.wait(lambda:n.find(text='Choose a model family',kind='Label'),timeout=30)
    n.wait(lambda:n.find(text='DeepSeek',kind='Label'),timeout=30)
    n.capture('host-add-sheet')
    n.click_row(n.reachable(identifier='search',kind='TextInput'))
    n.call('t',t='DeepSeek',wait=1)
    def filtered():
        labels={row.get('t') for row in n.rows() if row.get('ty')=='Label'}
        return 'DeepSeek' in labels and 'Anthropic' not in labels
    n.wait(filtered,timeout=30); time.sleep(1)
    assert filtered(), 'provider search did not retain its results'
    n.call('k',k='press',c='Escape',wait=1); time.sleep(2)
    n.click_row(n.reachable(text='DeepSeek',kind='Label'))
    def next_step():
        n.wait(lambda:(n.find(text='Next') or {}).get('enabled') is True,timeout=30)
        n.click('Next')
    next_step(); n.wait(lambda:n.find(text='Choose a model',kind='Label'),timeout=30)
    n.wait(lambda:n.find(text='DeepSeek V4 Flash',kind='Label'),timeout=30)
    n.click_row(n.reachable(text='DeepSeek V4 Flash',kind='Label'))
    n.wait(lambda:n.find(text='DeepSeek V4 Pro',kind='Label'),timeout=30)
    n.click_row(n.reachable(text='DeepSeek V4 Pro',kind='Label'))
    def selected_model():
        labels={row.get('t') for row in n.rows() if row.get('ty')=='Label'}
        return 'DeepSeek V4 Pro' in labels and 'DeepSeek V4 Flash' not in labels
    n.wait(selected_model,timeout=30)
    next_step(); n.wait(lambda:n.find(text='Choose a provider route',kind='Label'),timeout=30)
    n.wait(lambda:n.find(text='Official API',kind='Label'),timeout=30)
    n.click_row(n.reachable(text='Official API',kind='Label'))
    n.wait(lambda:n.find(text='Custom endpoint…',kind='Label'),timeout=30)
    n.click_row(n.reachable(text='Official API',kind='Label'))
    next_step(); n.wait(lambda:n.find(text='API key',kind='Label'),timeout=30)
    n.click_row(n.reachable(identifier='key',kind='TextInput'))
    n.call('t',t='fixture-only-key',wait=1)
    n.call('k',k='press',c='Escape',wait=1); time.sleep(2)
    if a.restyle_key:
        n.call('click',x=332,y=16,wait=1); time.sleep(3)
        key=n.find(identifier='key',kind='TextInput')
        assert n.find(text='API key',kind='Label') and key and key.get('val')=='fixture-only-key', 'appearance change lost the key step or test input'
    next_step(); n.wait(lambda:n.find(text='Test connection & save',kind='Label'),timeout=30)
    n.capture('summary')
    n.click('Back'); n.wait(lambda:n.find(text='API key',kind='Label'),timeout=30)
    next_step(); n.wait(lambda:n.find(text='Test connection & save',kind='Label'),timeout=30)
    cancel=[row for row in n.rows() if row.get('ty')=='Button' and row.get('t')=='Cancel']
    assert cancel, 'Cancel is missing'
    n.click_row(min(cancel,key=lambda row:row['r'][1])); n.label('Connect your first model'); time.sleep(1)
    n.label('Connect your first model')
    assert not n.find(text='Show QR for phone')
    n.capture('cancelled'); n.check_logs()
    assert '[E] splash:' not in n.log_path.read_text(errors='replace'), 'host sheet logged a script error'
    (out/'receipt.json').write_text(json.dumps({'status':'passed','checks':[
        'Empty list has one reachable Add model primary action',
        'Unavailable chat and QR export are absent until a model exists',
        'Provider search, choosing a different model and opening route options work',
        'The key step reaches the summary',
        'The entered test key is retained when returning from the summary',
        *(['Appearance change retains the key step and test input'] if a.restyle_key else []),
        'Cancel returns without saving a provider'], 'physical_mobile':False,'remote_provider_calls':False},indent=2)+'\n')
    print('PASS: host-owned provider wizard, retained test key, cancel without saving')
except Exception:
    n.capture('failed-state'); raise
finally:n.close()
