#!/usr/bin/env python3
"""Read-only source audit, called while the native build supervisor owns its lock."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import xml.etree.ElementTree as ET

exports = Path('/exports')
records = exports / 'systemui-build'
records.mkdir(exist_ok=True)
expected = json.loads((exports / 'installed-rom-expected-projects.json').read_text())
manifest = records / 'source-manifest.xml'
subprocess.run(['python3','/build/.repo/repo/repo','manifest','-r','-o',str(manifest)],cwd='/build',check=True)
actual = {p.get('path',p.get('name')):{'name':p.get('name'),'revision':p.get('revision'),'remote':p.get('remote','github')}
          for p in ET.parse(manifest).getroot().findall('project')}
assert actual == expected, 'Pinned ROM projects differ'
def changes(path):
    return path, subprocess.check_output(['git','-C','/build/'+path,'status','--porcelain'],text=True).splitlines()
with ThreadPoolExecutor(max_workers=4) as workers:
    dirty = {p:entries for p,entries in workers.map(changes,sorted(expected)) if entries}
assert set(dirty) == {'build/soong','packages/apps/Trebuchet','frameworks/base'}, dirty
assert dirty['build/soong'] == [' M ui/build/soong.go']
assert dirty['packages/apps/Trebuchet'] == [' M Android.bp','?? octosense/']
patch = json.loads((exports/'rom-soong-go-memory-environment.json').read_text())
assert hashlib.sha256(Path('/build/build/soong/ui/build/soong.go').read_bytes()).hexdigest() == patch['modified_sha256']
for name, manifest_name in [('mobile-source','mobile-source-manifest.json'),('systemui-source','systemui-source-manifest.json')]:
    inputs = exports/name
    hashes = {str(p.relative_to(inputs)):hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs.rglob('*') if p.is_file() and '__pycache__' not in p.parts}
    assert hashes == json.loads((exports/manifest_name).read_text()), name + ' inputs differ'
subprocess.run(['python3','/exports/mobile-source/android/platform-build/stage-quickstep.py','--tree','/build','--verify',
                '--baseline-result','/exports/upstream-build/quickstep-result.json'],check=True,stdout=subprocess.DEVNULL)
subprocess.run(['python3','/exports/systemui-source/android/platform-build/stage-systemui.py','--tree','/build','--verify',
                '--report',str(records/'staged-source-verification.json')],check=True)
record = {'status':'pass','projects':len(actual),'source_revisions_match_installed_manifest':True,
          'resolved_manifest_sha256':hashlib.sha256(manifest.read_bytes()).hexdigest(),
          'framework_revision':expected['frameworks/base']['revision'],'rom_product_build':False,'device_deployed':False}
(records/'source-verification.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
