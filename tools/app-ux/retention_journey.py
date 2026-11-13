#!/usr/bin/env python3
"""Keep six independent native Glance editors beyond the clean cache capacity."""
import argparse, hashlib, json, os, time
from pathlib import Path
from capture import ROOT
from calendar_journey import CalendarNative

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, default=ROOT/'target/release/octosense')
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
out = a.output.resolve(); out.mkdir(parents=True, exist_ok=True)
state = out/'state'
os.environ.update(OCTOSENSE_HOME=str(state/'home'), OCTOSENSE_APP_DATA=str(state/'apps'),
                  OCTOS_APP_CORE_DIR=str(state/'core'), RINX_DATA_DIR=str(state/'rinx'),
                  OCTOSENSE_LLM_VAULT='file')
fixtures = []
for i in range(6):
    fixtures.append({'app': 'test.uxretention', 'args': {
        'card_id': 'workspace-'+str(i), 'title': 'Workspace '+str(i),
        'summary': 'Fictional local state check', 'priority': 100-i, 'notify': False,
        'viewport': True,
        'script': (ROOT/'apps/interface.splash').read_text() + '\nView{width: Fill height: Fill flow: Down padding: 20 spacing: 16 '
                  'UiTitle{text: "Workspace '+str(i)+'" draw_text.color: theme.color_text} '
                  'draft := UiField{width: Fill height: 120 is_multiline: true empty_text: "Unsent local draft"}}'}})
fixture = out/'fixtures.json'; fixture.write_text(json.dumps(fixtures))
n = CalendarNative(a.binary.resolve(), ['--test-action', 'glance-fixtures:'+str(fixture)], out, 'retention')
checks=[]
def key(c): n.call('k', k='press', c=c, wait=1)
stacked = [False]
def open_at(index):
    mark = len(n.log_path.read_text())
    key('F9'); time.sleep(.2)
    for _ in range(12): key('ArrowUp')
    # The Mac's Notification Center stacks one app's cards: Return on the
    # stack expands it, and its cards follow its header. The panel logs its
    # cards once per change, so the last answer stands when it logs none.
    shown = [l for l in n.log_path.read_text()[mark:].splitlines() if 'glance panel: ' in l and ' card(s) ' in l]
    if shown: stacked[0] = shown[-1].count('test.uxretention@') == 1
    steps = index
    if stacked[0]:
        key('ReturnKey')
        n.wait(lambda: 'glance panel: expanded group test.uxretention' in n.log_path.read_text()[mark:])
        steps += 1
    for _ in range(steps): key('ArrowDown')
    key('ReturnKey')
    # Since this open: the second round would otherwise match the first's.
    n.wait(lambda: ('glance sheet: opened test.uxretention/workspace-'+str(index) in n.log_path.read_text()[mark:] or 'glance workspace: resume cached test.uxretention/workspace-'+str(index) in n.log_path.read_text()[mark:]))
    time.sleep(.2)
try:
    n.wait(lambda: 'glance fixture admission: Ok(6)' in n.log_path.read_text(), timeout=30)
    # A new publication auto-opens the pointer-only panel, except in the Mac's
    # style (a banner, and the column stays shut); close it first.
    log = n.log_path.read_text()
    if log.rfind('wm: glance panel open') > log.rfind('wm: glance panel closed'):
        key('F9')
    for i in range(6):
        open_at(i)
        n.call('click', x=700, y=265)
        n.call('t', t='Keep workspace '+str(i)+' · 未发送\nIndependent local state')
        n.capture('edited-'+str(i)); key('Escape')
    for i in [0, 5, 1, 4, 2, 3]:
        open_at(i)
        # Custom Glance script children are not exposed by /snap. Original
        # pixels must be visually reviewed; do not claim a machine text assertion.
        n.capture('restored-'+str(i)); checks.append(i); key('Escape')
    n.check_logs()
    (out/'receipt.json').write_text(json.dumps({'status':'captured_for_visual_review','workspaces':checks,
        'inactive_workspaces':5, 'clean_cache_capacity':3,
        'fixture_sha256':hashlib.sha256(fixture.read_bytes()).hexdigest(),
        'model_calls':False,'physical_mobile':False,'external_actions':False,
        'checks':['Six distinct local editors retain Unicode multiline unsent input after visiting five other workspaces']},indent=2)+'\n')
    print('CAPTURED: review all six restored editors against their distinct input')
except Exception:
    n.capture('failed-state'); raise
finally:
    n.close()
