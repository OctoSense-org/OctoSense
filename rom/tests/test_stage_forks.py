"""Exercise the build-host wrapper with a real Permission Git tree and stager."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from test_permissioncontroller_staging import BP, CONTRACT, INPUTS, MANIFEST, ROOT, STAGE


class StageForksTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.tree = self.root / 'tree'
        self.rom = self.root / 'rom'
        self.repo = self.tree / 'packages/modules/Permission'
        self.controller = self.repo / 'PermissionController'
        self.controller.mkdir(parents=True)
        (self.controller / 'AndroidManifest.xml').write_text(MANIFEST)
        (self.controller / 'Android.bp').write_text(BP)
        self.native = self.controller / 'src/com/android/permissioncontroller/Native.java'
        self.native.parent.mkdir(parents=True)
        self.native.write_text('// upstream native source\n')
        revision = self.init_repo(self.repo)
        for repo, path in [('frameworks/base', 'packages/SystemUI/Android.bp'),
                           ('packages/apps/Trebuchet', 'Android.bp')]:
            file = self.tree / repo / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text('// unrelated fork fixture\n')
            self.init_repo(self.tree / repo)

        scripts = self.rom / 'scripts'
        scripts.mkdir(parents=True)
        shutil.copy2(ROOT / 'scripts/stage-forks.sh', scripts)
        # Only PermissionController is under test; isolate the other integrations.
        (scripts / 'apply-to-tree.sh').write_text('#!/bin/bash\nexit 0\n')
        platform = self.rom / 'home/android/platform-build'
        platform.mkdir(parents=True)
        for script in ('stage-quickstep.py', 'stage-systemui.py'):
            (platform / script).write_text('# Unrelated stager fixture.\n')
        source = (ROOT / 'home/android/platform-build/stage-permissioncontroller.py').read_text()
        pin = "REVISION = '" + STAGE.REVISION + "'"
        self.assertEqual(source.count(pin), 1)
        (platform / 'stage-permissioncontroller.py').write_text(
            source.replace(pin, "REVISION = '" + revision + "'"))
        self.inputs = platform / 'permissioncontroller'
        shutil.copytree(INPUTS, self.inputs)
        for contract in (CONTRACT, CONTRACT.parent.parent / 'permissions/PermissionsSettingsContract.java'):
            target = self.rom / contract.relative_to(ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(contract, target)

        bin_dir = self.root / 'bin'
        bin_dir.mkdir()
        systemctl = bin_dir / 'systemctl'
        systemctl.write_text('#!/bin/sh\nexit 1\n')  # No build is running.
        systemctl.chmod(0o755)
        self.env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ['PATH'],
                        QUICKSTEP_BASELINE=str(self.root / 'quickstep-result.json'))

    def init_repo(self, repo):
        def git(*args):
            return subprocess.check_output(['git', '-C', str(repo), *args], text=True,
                                           stderr=subprocess.STDOUT).strip()
        git('init', '-q')
        git('add', '.')
        git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
            '-c', 'commit.gpgsign=false', '-c', 'core.hooksPath=/dev/null',
            'commit', '-qm', 'native fixture')
        return git('rev-parse', 'HEAD')

    def stage(self, success=True):
        result = subprocess.run(['bash', str(self.rom / 'scripts/stage-forks.sh'),
                                 str(self.tree), str(self.rom)], env=self.env,
                                capture_output=True, text=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('"mode": "verify"', result.stdout)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_restage_identical_and_updated_sources(self):
        self.stage()
        self.stage()
        backend = Path('src/com/android/permissioncontroller/octosense/RoleSettingsBackend.java')
        source = self.inputs / 'files' / backend
        source.write_text(source.read_text() + '\n// Updated adapter implementation.\n')
        manifest = self.inputs / 'components.xml'
        manifest.write_text(manifest.read_text().replace('</components>', '<!-- Updated integration. -->\n</components>'))
        self.stage()
        self.assertEqual((self.controller / backend).read_text(), source.read_text())
        self.assertIn('Updated integration.', (self.controller / 'AndroidManifest.xml').read_text())
        # An adapter removed from newer sources must also be removed from the tree.
        obsolete = self.controller / backend.parent / 'ObsoleteAdapter.java'
        obsolete.write_text('// previously staged adapter\n')
        self.stage()
        self.assertFalse(obsolete.exists())
        self.assertEqual(self.native.read_text(), '// upstream native source\n')

    def test_unrelated_tracked_and_untracked_edits_survive(self):
        self.stage()
        self.native.write_text('// unrelated local native edit\n')
        extra = self.repo / 'local-build-notes.txt'
        extra.write_text('unrelated local notes\n')
        result = self.stage(success=False)
        self.assertIn('Preserve unrelated Permission changes', result.stderr)
        self.assertEqual(self.native.read_text(), '// unrelated local native edit\n')
        self.assertEqual(extra.read_text(), 'unrelated local notes\n')
