import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('catalog_publish',Path(__file__).with_name('catalog_publish.py'))
m=importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class AuthorizationTests(unittest.TestCase):
    def setUp(self):
        self.env={'GITHUB_EVENT_NAME':'workflow_dispatch','GITHUB_REF':'refs/heads/main',
                  'GITHUB_REPOSITORY':m.REPOSITORY,'GITHUB_ACTOR':'maintainer',
                  'GITHUB_TRIGGERING_ACTOR':'second-admin','GITHUB_ACTOR_ID':'100'}
        self.repo={'id':m.REPOSITORY_ID,'owner':{'id':m.OWNER_ID},'private':False}
        self.permissions={'maintainer':{'permission':'admin','user':{'id':100}},
                          'second-admin':{'permission':'admin','user':{'id':101}}}
    def fetch(self,path):
        if path=='repos/'+m.REPOSITORY: return self.repo
        return self.permissions[path.split('/')[-2]]
    def test_verified_admin_and_admin_rerun(self):
        m.authorize(self.env,self.fetch)
    def test_write_permission_does_not_authorize_catalog_signing(self):
        for actor in self.permissions:
            with self.subTest(actor=actor):
                self.permissions[actor]['permission']='write'
                with self.assertRaises(m.Refused): m.authorize(self.env,self.fetch)
                self.permissions[actor]['permission']='admin'
    def test_account_name_cannot_replace_numeric_identity(self):
        self.permissions['maintainer']['user']['id']=999
        with self.assertRaises(m.Refused): m.authorize(self.env,self.fetch)
    def test_fork_pr_or_nonmain_workflow_cannot_sign(self):
        for key,value in [('GITHUB_EVENT_NAME','pull_request_target'),('GITHUB_REF','refs/heads/evil'),
                          ('GITHUB_REPOSITORY','attacker/App-Hub'),('GITHUB_TRIGGERING_ACTOR','')]:
            env=dict(self.env);env[key]=value
            with self.subTest(key=key),self.assertRaises(m.Refused): m.authorize(env,self.fetch)
    def test_repository_transfer_and_private_sigstore_instance_refused(self):
        for mutation in [dict(id=1),dict(owner={'id':1}),dict(private=True)]:
            original=copy.deepcopy(self.repo);self.repo.update(mutation)
            with self.assertRaises(m.Refused): m.authorize(self.env,self.fetch)
            self.repo=original


class CandidateTests(unittest.TestCase):
    def test_no_shell_fragments_or_traversal_in_inputs(self):
        for value in ['../other','a/b','a;touch bad','$(id)','-option','a\nb']:
            with self.subTest(value=value),self.assertRaises(m.Refused):m.inputs('a'*40,value,'b'*64)
        for value in ['../x','/x','a/../x','a\\x','a/.git/config','a//x','a/./x','a\nx']:
            with self.subTest(value=value),self.assertRaises(m.Refused):m.safe_path(value)
    def test_only_reviewed_bytes_extracted_no_candidate_script_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);repo=root/'repo';repo.mkdir()
            subprocess.run(['git','init','-q',str(repo)],check=True)
            source=repo/'catalog-candidates/fixture';source.mkdir(parents=True)
            (source/'catalog.json').write_text('{}')
            (source/'build.sh').write_text('exit 91\n')
            subprocess.run(['git','add','.'],cwd=repo,check=True)
            subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid',
                            'commit','-qm','Synthetic fixture'],cwd=repo,check=True)
            commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
            m.extract_candidate(repo,commit,'fixture',root/'extracted')
            self.assertEqual((root/'extracted/build.sh').read_text(),'exit 91\n')
            (source/'escape').symlink_to('/etc/passwd')
            subprocess.run(['git','add','.'],cwd=repo,check=True)
            subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid',
                            'commit','-qm','Link refusal fixture'],cwd=repo,check=True)
            commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
            with self.assertRaises(m.Refused):m.extract_candidate(repo,commit,'fixture',root/'refused')


class ReceiptTests(unittest.TestCase):
    def test_app_and_component_receipts_list_exactly_their_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            (root/'artifacts/app-1.bundle').mkdir(parents=True)
            (root/'artifacts/app-1.bundle/main.splash').write_text('View{}')
            (root/'artifacts/app-1.bundle.pack.json').write_text('{}')
            (root/'index/components').mkdir(parents=True)
            (root/'index/app-1.json').write_text('{}')
            (root/'artifacts/org.example.markdown-1.0.0.wasm').write_bytes(b'\0asm\r\0\x01\0')
            (root/'index/components/org.example.markdown-1.0.0.json').write_text('{}')
            native={'added_artifacts':[
                {'bundle':'artifacts/app-1.bundle','pack':'artifacts/app-1.bundle.pack.json','index':'index/app-1.json'},
                {'component':'artifacts/org.example.markdown-1.0.0.wasm','index':'index/components/org.example.markdown-1.0.0.json'}]}
            self.assertEqual(m.listed_files(root,native),sorted([
                'artifacts/app-1.bundle/main.splash','artifacts/app-1.bundle.pack.json','index/app-1.json',
                'artifacts/org.example.markdown-1.0.0.wasm','index/components/org.example.markdown-1.0.0.json']))
            for bad in [{'component':'artifacts/org.example.markdown-1.0.0.wasm'},
                        {'component':'index/org.example.markdown-1.0.0.wasm','index':'index/components/org.example.markdown-1.0.0.json'},
                        {'component':'artifacts/app-1.bundle.pack.json','index':'index/app-1.json'},
                        {'component':'artifacts/org.example.markdown-1.0.0.wasm','index':'artifacts/org.example.markdown-1.0.0.wasm'},
                        {'component':'artifacts/missing-1.0.0.wasm','index':'index/app-1.json'},
                        {'component':'artifacts/org.example.markdown-1.0.0.wasm','index':'index/app-1.json','pack':'artifacts/app-1.bundle.pack.json'}]:
                with self.subTest(bad=bad),self.assertRaises(m.Refused):m.listed_files(root,{'added_artifacts':[bad]})


class PacketTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.addCleanup(self.temp.cleanup)
        root=Path(self.temp.name);self.repo=root/'repo';self.packet=root/'packet'
        self.repo.mkdir();self.packet.mkdir()
        (self.repo/'catalog.json').write_text('{"sequence":1}')
        subprocess.run(['git','init','-q','-b','main',str(self.repo)],check=True)
        subprocess.run(['git','add','.'],cwd=self.repo,check=True)
        subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid',
                        'commit','-qm','Synthetic baseline'],cwd=self.repo,check=True)
        self.head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=self.repo,text=True).strip()
        (self.packet/m.SUBJECT).write_text('{"sequence":2}')
        self.relative='artifacts/fixture-1.bundle/main.splash'
        self.artifact=self.packet/'publication'/self.relative
        self.artifact.parent.mkdir(parents=True);self.artifact.write_text('View{}')
        self.approved=m.digest(self.packet/m.SUBJECT)
        self.receipt={'schema':1,'repository_id':m.REPOSITORY_ID,'owner_id':m.OWNER_ID,
            'workflow_commit':self.head,'base_catalog':'catalog.json','base_sha256':m.digest(self.repo/'catalog.json'),
            'approved_sha256':self.approved,'files':{self.relative:m.digest(self.artifact)},'native':{'sequence':2}}
        (self.packet/'receipt.json').write_text(json.dumps(self.receipt))
        self.environment=patch.dict(os.environ,{'GITHUB_SHA':self.head});self.environment.start()
        self.addCleanup(self.environment.stop)
    def check(self,head=None):
        return m.recheck(self.repo,self.packet,self.approved,lambda _: {'object':{'sha':head or self.head}})
    def test_exact_packet_accepted(self):self.check()
    def test_stale_main_refused_without_retry(self):
        with self.assertRaisesRegex(m.Refused,'Main advanced'):self.check('b'*40)
    def test_changed_payload_base_or_artifact_refused(self):
        for path in [self.packet/m.SUBJECT,self.repo/'catalog.json',self.artifact]:
            previous=path.read_bytes();path.write_bytes(previous+b' ')
            with self.subTest(path=path.name),self.assertRaises(m.Refused):self.check()
            path.write_bytes(previous)
    def test_unlisted_file_cannot_enter_commit(self):
        (self.artifact.parent/'extra').write_text('unreviewed')
        with self.assertRaises(m.Refused):self.check()
    def test_immutable_artifact_cannot_be_replaced(self):
        path=self.repo/self.relative;path.parent.mkdir(parents=True);path.write_text('existing')
        with self.assertRaisesRegex(m.Refused,'immutable'):self.check()
    def test_symlinked_parent_refused_even_when_bytes_match(self):
        parent=self.artifact.parent;moved=parent.with_name('moved');parent.rename(moved);parent.symlink_to(moved)
        with self.assertRaises(m.Refused):self.check()
    def test_wrong_checkout_or_dirty_tracked_file_refused(self):
        with patch.dict(os.environ,{'GITHUB_SHA':'b'*40}),self.assertRaises(m.Refused):self.check()
        (self.repo/'catalog.json').write_text('{"sequence":99}')
        with self.assertRaises(m.Refused):self.check()
    def test_missing_cli_packet_fails_cleanly(self):
        result=subprocess.run(['python3',str(Path(m.__file__)),'publish'],capture_output=True,text=True)
        self.assertEqual(result.returncode,1)
        self.assertIn('requires --packet',result.stderr)
        self.assertNotIn('Traceback',result.stderr)
    def test_real_git_transaction_refuses_concurrent_main_and_preserves_legacy(self):
        origin=Path(self.temp.name)/'origin.git'
        subprocess.run(['git','init','-q','--bare',str(origin)],check=True)
        subprocess.run(['git','remote','add','origin',str(origin)],cwd=self.repo,check=True)
        subprocess.run(['git','push','-q','origin','HEAD:refs/heads/main'],cwd=self.repo,check=True)
        concurrent=Path(self.temp.name)/'concurrent'
        subprocess.run(['git','clone','-q','-b','main',str(origin),str(concurrent)],check=True)
        (concurrent/'unrelated.txt').write_text('concurrent change')
        subprocess.run(['git','add','.'],cwd=concurrent,check=True)
        subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid',
                        'commit','-qm','Concurrent main update'],cwd=concurrent,check=True)
        updated=subprocess.check_output(['git','rev-parse','HEAD'],cwd=concurrent,text=True).strip()
        real_command=m.command;real_recheck=m.recheck
        # Only emulate the independently tested native proof verifier. All Git
        # staging/commit/push operations and catalog packet checks remain real.
        def invoke(args,cwd=None,env=None):
            if args[0]=='fixture-hub':
                if args[1]=='catalog-envelope':(self.packet/'catalog-v2.json').write_text('verified envelope fixture')
                return b'{}'
            if args[:3]==['git','-c','core.hooksPath=/dev/null'] and 'push' in args:
                subprocess.run(['git','push','-q','origin','HEAD:refs/heads/main'],cwd=concurrent,check=True)
            return real_command(args,cwd,env)
        def recheck(repo,packet,approved):
            return real_recheck(repo,packet,approved,lambda _: {'object':{'sha':self.head}})
        with patch.object(m,'authorize'),patch.object(m,'command',side_effect=invoke),\
                patch.object(m,'recheck',side_effect=recheck),patch.dict(os.environ,{'GH_TOKEN':'synthetic-job-token'}):
            dry=m.publish(self.repo,Path('fixture-hub'),self.packet,Path('proof'),self.approved,True)
            self.assertEqual(dry,{'signed':True,'published':False,'sequence':2})
            self.assertFalse((self.repo/'catalog-v2.json').exists())
            with self.assertRaisesRegex(m.Refused,'Trusted command failed'):
                m.publish(self.repo,Path('fixture-hub'),self.packet,Path('proof'),self.approved,False)
        self.assertEqual(subprocess.check_output(['git','--git-dir',str(origin),'rev-parse','refs/heads/main'],text=True).strip(),updated)
        self.assertEqual((self.repo/'catalog.json').read_text(),'{"sequence":1}')
        self.assertEqual(subprocess.check_output(['git','diff','--name-only',self.head,'HEAD'],cwd=self.repo,text=True).splitlines(),
                         [self.relative,'catalog-v2.json'])


if __name__=='__main__':unittest.main()
