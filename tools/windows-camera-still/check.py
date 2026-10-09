#!/usr/bin/env python3
"""Compile/test the actual Windows still worker portably; no camera/OS proof."""
import argparse, hashlib, json, os, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    out=args.output.resolve();out.mkdir(parents=True,exist_ok=False,mode=0o700)
    runtime=ROOT/'.sources/makepad';source=runtime/'platform/src/os/windows/camera_still.rs';mf=runtime/'platform/src/os/windows/media_foundation.rs'
    report={'result':'failed','not_verified':['Windows target ABI','Media Foundation device callbacks','OS permission','camera hardware','per-app quota reservation']}
    try:
        layout=mf.read_text().split('pub(super) fn camera_frame_from_media_buffer',1)[1].split('\nstruct MfInput',1)[0]
        harness=(Path(__file__).parent/'harness.rs').read_text().replace('__FRAME_LAYOUT__','pub(super) fn camera_frame_from_media_buffer'+layout).replace('__WORKER_PATH__',json.dumps(str(source)))
        (out/'harness.rs').write_text(harness)
        commands=[]
        for name,lib,extra in [
            ('makepad_zune_core','libs/zune/zune-core/src/lib.rs',[]),
            ('jpeg_encoder','libs/jpeg-encoder/src/lib.rs',[]),
            ('makepad_zune_jpeg','libs/zune/zune-jpeg/src/lib.rs',['--extern',f'makepad_zune_core={out}/libmakepad_zune_core.rlib'])]:
            commands.append(['rustc','--crate-name',name,'--crate-type','rlib','--edition=2021','-O','--cfg','feature="std"',str(runtime/lib),'--out-dir',str(out),'-L','dependency='+str(out),*extra])
        commands.append(['rustc','--test','--edition=2021','-O',str(out/'harness.rs'),'-o',str(out/'still-tests'),'-L','dependency='+str(out),'--extern',f'jpeg_encoder={out}/libjpeg_encoder.rlib','--extern',f'makepad_zune_jpeg={out}/libmakepad_zune_jpeg.rlib'])
        commands.append([str(out/'still-tests'),'--test-threads=1','--nocapture'])
        env=dict(os.environ,OCTOSENSE_STILL_TEST_OUTPUT=str(out))
        for index,command in enumerate(commands):
            with (out/f'{index}.log').open('w') as log:done=subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT)
            if done.returncode:raise RuntimeError(f'Phase {index} failed; inspect {index}.log')
        report.update(result='pass',tests=9,worker_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),reader_sha256=hashlib.sha256(mf.read_bytes()).hexdigest(),harness_sha256=hashlib.sha256((out/'harness.rs').read_bytes()).hexdigest())
    except Exception as error:report['error']=str(error)
    (out/'result.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report));return 0 if report['result']=='pass' else 1
if __name__=='__main__':raise SystemExit(main())
