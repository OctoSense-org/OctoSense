#!/usr/bin/env python3
"""Render exact panel sources with owned fixture data; never capture the display.

The isolated preview supplies no system privileges, Bridge binding, or real
notifications. Restores rotation and removes the temporary preview application.
"""
import argparse
import hashlib
import importlib
import json
from pathlib import Path
import shlex
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--apk', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    phone = importlib.import_module('run-global-shade-validation').Phone(args.adb, args.serial)
    package = 'dev.makepad.octosense.shadepreview'
    report = {'result': 'fail', 'captures': [], 'apk_sha256': hashlib.sha256(args.apk.read_bytes()).hexdigest()}
    rotation = {}
    installed = False
    cases = [
        ('controls-dark', 'controls', {}), ('controls-light', 'controls', {'light':'true'}),
        ('notifications-dark', 'notifications', {}), ('notifications-light', 'notifications', {'light':'true'}),
        ('reply-dark', 'notifications', {'action':'Reply'}), ('reply-light', 'notifications', {'action':'Reply','light':'true'}),
        ('empty', 'empty', {}), ('permissions', 'permissions', {}), ('automatic', 'automatic', {}),
        ('settings', 'controls', {'action':'System setup'}), ('onboarding', 'onboarding', {}),
        ('large-type', 'controls', {'font_scale':'2'}), ('rtl', 'controls', {'rtl':'true'}),
        ('landscape-dark', 'controls', {'rotation':'1'}), ('landscape-light', 'controls', {'rotation':'3','light':'true'}),
        ('reply-large-type', 'notifications', {'action':'Reply','font_scale':'2'}),
        ('reply-landscape', 'notifications', {'action':'Reply','rotation':'1'})]
    try:
        rotation = {k: phone.command('shell','settings','get','system',k) for k in ['accelerometer_rotation','user_rotation']}
        assert 'Success' in phone.command('install','--no-incremental','-r',str(args.apk)); installed = True
        phone.wake_home()
        phone.command('shell','settings','put','system','accelerometer_rotation','0')
        for name, scenario, options in cases:
            phone.command('shell','settings','put','system','user_rotation',options.get('rotation','0'))
            time.sleep(.5)
            values={'name':name,'scenario':scenario,**{k:v for k,v in options.items() if k!='rotation'}}
            command = 'am instrument -w ' + ' '.join('-e '+shlex.quote(k)+' '+shlex.quote(v) for k,v in values.items())
            command += ' '+package+'/dev.makepad.octosense.quickstep.ShadeDesignInstrumentation'
            result = phone.command('shell',command)
            (args.output/(name+'.txt')).write_text(result+'\n')
            assert 'result=pass' in result, result
            destination=args.output/(name+'.png')
            subprocess.run(phone.adb+['pull','/sdcard/Android/data/'+package+'/files/design/'+name+'.png',str(destination)],check=True,capture_output=True)
            report['captures'].append({'name':name,'path':str(destination),'fixture_data_only':True,'view_draw':True,'small_touch_targets':0})
            print('Passed '+name,flush=True)
        report['result']='pass'
    except Exception as error:
        report['failure']=str(error)
    finally:
        try:
            for key in ['user_rotation','accelerometer_rotation']:
                if key in rotation:
                    if rotation[key]=='null': phone.command('shell','settings','delete','system',key)
                    else: phone.command('shell','settings','put','system',key,rotation[key])
            assert all(phone.command('shell','settings','get','system',k)==v for k,v in rotation.items())
            report['rotation_restored']=True
            if installed:
                assert 'Success' in phone.command('uninstall',package)
                report['preview_removed']=True
            phone.wake_home()
        except Exception as error:
            report['result']='fail'; report['cleanup_error']=str(error)
        (args.output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='captures'},indent=2))
    return 0 if report['result']=='pass' else 1


if __name__=='__main__': raise SystemExit(main())
