#!/usr/bin/env python3
"""Package a signer-compatible SystemUI APK for the reviewed OnePlus ROM only.

Never installs. An incompatible candidate produces a blocked receipt, no module
ZIP. There is no switch for bypassing the installed release signer requirement.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import zipfile

ORIGINAL_APK = 'cf934c396775e84e10354fc787b40f756ce7abecd079b72558f8eebbc0ab55f7'
# Keep this explicit rather than accepting an arbitrary APK as the reference.
FRAMEWORK_APK = 'f23f0e7d35876801cb4eb4bf318496db10561c73ca51af408ce066c74ea34b03'
MODULE = 'octosense_systemui_enchilada'

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apk',type=Path,required=True);parser.add_argument('--installed-apk',type=Path,required=True)
    parser.add_argument('--aapt',required=True);parser.add_argument('--apksigner',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    assert sha(args.installed_apk)==ORIGINAL_APK, 'Reference differs from reviewed phone SystemUI'
    result=subprocess.run([sys.executable,str(Path(__file__).with_name('inspect-systemui-apk.py')),
        '--apk',str(args.apk),'--installed-apk',str(args.installed_apk),'--aapt',args.aapt,
        '--apksigner',args.apksigner,'--output',str(args.output/'inspection.json'),'--require-installed-signer'],capture_output=True,text=True)
    report={'status':'blocked','apk_sha256':sha(args.apk),'installed_apk_sha256':ORIGINAL_APK,'module_created':False,'phone_deployed':False}
    if result.returncode:
        inspection=args.output/'inspection.json'
        report['inspection']=json.loads(inspection.read_text()) if inspection.exists() else {'error':result.stderr}
        (args.output/'module-result.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'status':'blocked','reason':'APK must pass manifest checks and match the installed release signer','module_created':False},indent=2))
        return 2
    staging=args.output/'payload';staging.mkdir()
    apk=staging/'system/system_ext/priv-app/SystemUI/SystemUI.apk';apk.parent.mkdir(parents=True);shutil.copy2(args.apk,apk)
    (staging/'module.prop').write_text('id='+MODULE+'\nname=OctoSense System Interface (OnePlus 6)\nversion=1.0\nversionCode=1\nauthor=OctoSense\ndescription=ROM-matched native SystemUI; preserves the current Home and Recents provider.\n')
    customize='''#!/system/bin/sh
[ "$BOOTMODE" = true ] || abort "Install only from the booted Android system."
[ "$MAGISK_VER_CODE" -ge 29000 ] || abort "Magisk 29 or later required."
[ "$API" = 35 ] && [ "$ARCH" = arm64 ] || abort "Android 15 ARM64 required."
[ "$(getprop ro.lineage.device)" = enchilada ] || abort "Reviewed OnePlus 6 required."
[ "$(getprop ro.lineage.version)" = 22.2-20260708-NIGHTLY-enchilada ] || abort "ROM version differs."
[ "$(getprop ro.build.version.incremental)" = 21be58cea4 ] || abort "ROM incremental differs."
[ ! -d /data/adb/modules/octosense_systemui_enchilada ] || abort "An existing SystemUI module needs a new upgrade review."
[ "$(sha256sum /system/framework/framework-res.apk | cut -d ' ' -f 1)" = FRAMEWORK_SHA ] || abort "Framework differs."
[ "$(sha256sum /system/system_ext/priv-app/SystemUI/SystemUI.apk | cut -d ' ' -f 1)" = ORIGINAL_SHA ] || abort "Installed SystemUI differs."
(cd "$MODPATH" && sha256sum -c payload.sha256) || abort "Payload checksum failed."
set_perm_recursive "$MODPATH" 0 0 0755 0644
ui_print "Prepared signer-compatible OctoSense SystemUI. Reboot only with the reviewed recovery path available."
ui_print "Rollback: disable only octosense_systemui_enchilada and reboot; keep the Quickstep module active."
'''.replace('FRAMEWORK_SHA',FRAMEWORK_APK).replace('ORIGINAL_SHA',ORIGINAL_APK)
    (staging/'customize.sh').write_text(customize)
    subprocess.run(['sh','-n',str(staging/'customize.sh')],check=True)
    payload={str(p.relative_to(staging)):sha(p) for p in staging.rglob('*') if p.is_file()}
    (staging/'payload.sha256').write_text(''.join(digest+'  '+name+'\n' for name,digest in sorted(payload.items())))
    archive=args.output/(MODULE+'.zip')
    with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as zipped:
        for file in sorted(staging.rglob('*')):
            if file.is_file():zipped.write(file,str(file.relative_to(staging)))
    report.update(status='packaged',module_created=True,module_sha256=sha(archive),payload=payload,
        phone_runtime_validated=False,rollback_tested=False)
    (args.output/'module-result.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2));return 0

if __name__=='__main__':raise SystemExit(main())
