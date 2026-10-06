#!/usr/bin/env python3
"""Exercise actual native editor events, exact text, styles and table picker."""
import argparse, hashlib, json, sys, tempfile
from pathlib import Path
root = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(root/'tools/connected-e2e'))
from native import Native
parser = argparse.ArgumentParser(description='Provider-free native Rinx writer checks')
parser.add_argument('--host', type=Path, default=root/'target/release/examples/editor-host')
parser.add_argument('--output', type=Path, default=root/'target/markdown-writer-native')
args = parser.parse_args()
r = root
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
fixture = Path(__file__).with_name('fixtures')/'reference.splash'
binary_sha = hashlib.sha256(args.host.read_bytes()).hexdigest()
cases = []
for name,extra in [('wide',['--wide']),('narrow',[])]:
 ui=Native(args.host.resolve(),['--source='+str(fixture),'--app-data='+str(out/name),*extra],out,'verify-'+name)
 try:
  ui.wait(lambda:'A quieter morning' in (ui.find(identifier='markdown',kind='TextInput') or {}).get('t',''))
  click=lambda i:ui.click_row(ui.wait(lambda:ui.find(identifier=i)))
  text='# Café 中文 🦀\n\nFirst paragraph.\n\n## Another section\n\n- One\n- Two\n'
  def field(value):
   row=ui.find(identifier='markdown',kind='TextInput');x,y,w,h=row['r'];ui.call('click',x=x+32,y=y+24,wait=1);ui.call('k',k='press',c='KeyA',cmd=1,wait=1);ui.call('t',t=value,wait=1);ui.wait(lambda:ui.find(identifier='markdown',kind='TextInput')['t']==value)
  field(text);ui.wait(lambda:(out/name/'draft.md').read_text()==text)
  click('preview_mode');ui.capture(name+'-preview');click('source_mode')
  assert ui.find(identifier='markdown',kind='TextInput')['t']==text
  field('Unicode 你好 café')
  ui.call('k',k='press',c='KeyA',cmd=1,wait=1)
  click('bold' if name=='wide' else 'mobile_bold')
  expected='**Unicode 你好 café**'
  ui.wait(lambda:(out/name/'draft.md').read_text()==expected)
  click('preview_mode');click('palette_button' if name=='wide' else 'style_fab')
  ui.capture(name+'-styles')
  click('undo_button' if name=='wide' else 'mobile_undo_button')
  ui.wait(lambda:(out/name/'draft.md').read_text()=='Unicode 你好 café')
  click('redo_button' if name=='wide' else 'mobile_redo_button')
  ui.wait(lambda:(out/name/'draft.md').read_text()==expected)
  click('rich_mode' if name=='wide' else 'mobile_rich_mode')
  ui.wait(lambda:ui.find(identifier='rich',kind='ArticleRichInput'))
  ui.capture(name+'-rich')
  click('source_mode');assert ui.find(identifier='markdown',kind='TextInput')['t']==expected
  click('table' if name=='wide' else 'mobile_table');ui.capture(name+'-table')
  picker=ui.find(identifier='table_picker',kind='TableSizePicker');assert picker
  x,y,_,_=picker['r'];ui.call('click',x=x+34,y=y+34,wait=1)
  ui.wait(lambda:'| Column 1 | Column 2 |' in (out/name/'draft.md').read_text())
  ui.capture(name+'-table-inserted')
  ui.check_logs();cases.append({'name':name,'passed':True,'checks':['Unicode exact readback','source/preview retention','selection Bold source bytes','undo/redo exact source','secondary rich-mode retention','native table-size picker insertion']})
 finally:ui.close()
assert hashlib.sha256(args.host.read_bytes()).hexdigest() == binary_sha, 'Binary changed during validation'
receipt = {'scope': 'provider-free reusable writer native UI; not installed-app or Android acceptance', 'binary_sha256': binary_sha, 'cases': cases, 'owned_processes_stopped': True}
(out/'functional.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(receipt))
