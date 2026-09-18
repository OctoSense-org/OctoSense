#!/usr/bin/env python3
"""Test the exact native controls activity under an isolated, temporary app UID.

Does not replace SystemUI, grant system settings access, or claim coverage of
keyguard/navigation services. Uses owned UI only and restores volume on exit.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import time

spec=importlib.util.spec_from_file_location('phone',Path(__file__).with_name('run-global-shade-validation.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb',required=True);parser.add_argument('--serial',required=True)
    parser.add_argument('--apk',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    package='dev.makepad.octosense.systemuipreview'
    component=package+'/com.android.systemui.octosense.OctoSenseSystemActivity'
    phone=module.Phone(args.adb,args.serial,owned_focus=[package+'/'])
    result={'result':'fail','checks':{},'scope':'Isolated native controls activity, not SystemUI service runtime'}
    installed=False;original=None
    def volume():
        observed=phone.command('shell','cmd','media_session','volume','--stream','3','--get')
        return int(re.search(r'volume is (\d+)',observed).group(1))
    def launch():
        phone.command('shell','am','start','-n',component);phone.wait_focus(component);time.sleep(.8)
    try:
        assert not phone.command('shell','pm','list','packages','--user','0',package), 'Preserve existing preview installation'
        original=volume();result['original_volume']=original
        assert 'Success' in phone.command('install','--no-incremental',str(args.apk));installed=True
        phone.wake_home();launch()
        tree=phone.tree();phone.node(text='OCTOSENSE',tree=tree);phone.node(text='Device controls',tree=tree)
        assert phone.node(description='Brightness',tree=tree).get('enabled')=='false'
        assert phone.node(text='Rotation lock',tree=tree).get('enabled')=='false'
        result['checks']['ungranted_settings_controls_disabled']=True
        slider=phone.node(description='Media volume',tree=tree)
        left,top,right,bottom=map(int,re.findall(r'\d+',slider.get('bounds')))
        phone.command('shell','input','tap',str(round(left+(right-left)*.7)),str((top+bottom)//2));time.sleep(.5)
        assert volume()!=original
        result['checks']['native_media_volume_changed']=True
        phone.command('shell',"su -c 'cmd media_session volume --stream 3 --set "+str(original)+"'")
        assert volume()==original
        result['checks']['media_volume_restored']=True
        for label,target in [('Your connection','SystemUIDialog'),('Wi-Fi networks','Settings$WifiSettingsActivity'),('Pair Bluetooth','Settings$ConnectedDeviceDashboardActivity'),('OctoSense permission setup','BridgeSettingsActivity')]:
            launch()
            for attempt in range(5):
                tree=phone.tree();found=[n for n in tree.iter('node') if n.get('text')==label]
                if len(found)==1:
                    phone.tap(found[0]);break
                phone.command('shell','input','swipe','540','1600','540','650','350')
            else: raise AssertionError('Control not found: '+label)
            phone.wait_focus(target)
            result['checks']['route_'+label]=True
            # Dismiss Internet's dialog before starting another activity.
            if target=='SystemUIDialog': phone.command('shell','input','keyevent','4')
        launch();old_pid=phone.command('shell','pidof',package)
        phone.command('shell','am','force-stop',package);launch()
        assert old_pid!=phone.command('shell','pidof',package)
        phone.node(description='Media volume')
        result['checks']['activity_restarts_from_observed_state']=True
        phone.tap(phone.node(text='Close'))
        assert package+'/' not in phone.focus()
        result['checks']['close_dismisses_controls']=True
        result['result']='pass'
    except Exception as error:
        result['failure']=str(error) or repr(error)
    finally:
        if original is not None:
            try:
                phone.command('shell',"su -c 'cmd media_session volume --stream 3 --set "+str(original)+"'")
                assert volume()==original;result['volume_restored']=True
            except Exception as error:result.update(result='fail',restore_error=str(error))
        if installed:
            try:
                assert 'Success' in phone.command('uninstall',package)
                assert not phone.command('shell','pm','list','packages','--user','0',package)
                result['preview_removed']=True
                phone.wake_home()
            except Exception as error:result.update(result='fail',cleanup_error=str(error))
        (args.output/'results.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2));return 0 if result['result']=='pass' else 1

if __name__=='__main__':raise SystemExit(main())
