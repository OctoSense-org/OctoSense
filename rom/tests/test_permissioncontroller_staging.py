"""A role adapter may extend only the reviewed PermissionController source."""
import fcntl
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('stage_roles', ROOT / 'home/android/platform-build/stage-permissioncontroller.py')
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)
INPUTS = ROOT / 'home/android/platform-build/permissioncontroller'
CONTRACT = ROOT / 'vendor/octosense/settings/src/dev/makepad/octosense/roles/RolesSettingsContract.java'
A = '{http://schemas.android.com/apk/res/android}'
MANIFEST = '''<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.android.permissioncontroller">
    <uses-permission android:name="android.permission.MANAGE_ROLE_HOLDERS" />
    <application android:label="@string/app_name">
        <activity android:name=".NativePicker" android:exported="true" />
    </application>
</manifest>
'''
BP = 'android_library {\n    name: "PermissionController-lib",\n    srcs: [\n        "src/**/*.java",\n        "src/**/*.kt",\n    ],\n}\n'


class PermissionControllerStagingTest(unittest.TestCase):
    def test_only_gated_service_and_unexported_confirmation_extend_native_package(self):
        changes = STAGE.generate(lambda path: MANIFEST if path == 'AndroidManifest.xml' else BP, INPUTS, CONTRACT)
        manifest = ET.fromstring(changes['AndroidManifest.xml'])
        self.assertIsNone(manifest.get(A + 'sharedUserId'))
        self.assertEqual([node.get(A + 'name') for node in manifest.findall('uses-permission')], ['android.permission.MANAGE_ROLE_HOLDERS'])
        application = manifest.find('application')
        self.assertEqual(application.find('activity').get(A + 'name'), '.NativePicker')
        added = list(application)[1:]
        self.assertEqual([node.tag for node in added], ['service', 'activity', 'service', 'activity'])
        service, confirmation, permissions, operation = added
        self.assertEqual(service.get(A + 'permission'), 'dev.makepad.octosense.permission.BIND_AGENT_PLATFORM')
        self.assertEqual(service.get(A + 'exported'), 'true')
        self.assertEqual(confirmation.get(A + 'exported'), 'false')
        self.assertEqual(confirmation.get(A + 'theme'), '@style/RequestRole.FilterTouches')
        self.assertEqual(permissions.get(A + 'permission'), 'dev.makepad.octosense.permission.BIND_AGENT_PLATFORM')
        self.assertEqual(permissions.get(A + 'exported'), 'true')
        self.assertEqual(operation.get(A + 'exported'), 'false')
        self.assertEqual(operation.get(A + 'theme'), '@style/RequestRole.FilterTouches')
        self.assertEqual(application.findall('.//intent-filter'), [])
        self.assertIn('src/com/android/permissioncontroller/octosense/IRoleSettings.aidl', changes['Android.bp'])
        self.assertIn('src/com/android/permissioncontroller/octosense/IPermissionSettings.aidl', changes['Android.bp'])
        # No native Java, resources, role descriptors or permissions are patched.
        expected = {'AndroidManifest.xml', 'Android.bp', 'src/dev/makepad/octosense/roles/RolesSettingsContract.java', 'src/dev/makepad/octosense/permissions/PermissionsSettingsContract.java'}
        expected.update(str(path.relative_to(INPUTS / 'files')) for path in (INPUTS / 'files').rglob('*') if path.is_file())
        self.assertEqual(set(changes), expected)

    def test_read_only_check_exact_stage_revision_dirty_source_and_lock(self):
        with tempfile.TemporaryDirectory() as output:
            tree = Path(output)
            repo = tree / 'packages/modules/Permission'
            controller = repo / 'PermissionController'
            controller.mkdir(parents=True)
            (controller / 'AndroidManifest.xml').write_text(MANIFEST)
            (controller / 'Android.bp').write_text(BP)
            def git(*args):
                return subprocess.check_output(['git', '-C', str(repo), *args], text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '-q')
            git('add', '.')
            git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'native fixture')
            revision, STAGE.REVISION = STAGE.REVISION, git('rev-parse', 'HEAD')
            try:
                report = tree / 'result.json'
                STAGE.stage(tree, INPUTS, CONTRACT, report, check=True)
                self.assertEqual(git('status', '--porcelain'), '')
                with (tree / '.octosense-build.lock').open('a') as lock:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    with self.assertRaises(BlockingIOError):
                        STAGE.stage(tree, INPUTS, CONTRACT, report)
                self.assertEqual(git('status', '--porcelain'), '')
                # Exact earlier roles-only staging is a reviewed migration;
                # even one unrelated manifest byte must still be preserved.
                components = (INPUTS / 'components.xml').read_text()
                role_nodes = components[components.index('>') + 1:components.index('    <service android:name="com.android.permissioncontroller.octosense.OctoSensePermissionsService"')]
                previous = MANIFEST.replace('    </application>', role_nodes + '\n    </application>')
                (controller / 'AndroidManifest.xml').write_text(previous + '<!-- unrelated -->')
                with self.assertRaisesRegex(RuntimeError, 'Preserve modified'):
                    STAGE.stage(tree, INPUTS, CONTRACT, report, check=True)
                (controller / 'AndroidManifest.xml').write_text(previous)
                (controller / 'Android.bp').write_text(BP.replace('        "src/**/*.kt",', '        "src/**/*.kt",\n        "src/com/android/permissioncontroller/octosense/IRoleSettings.aidl",'))
                STAGE.stage(tree, INPUTS, CONTRACT, report, check=True)
                self.assertEqual((controller / 'AndroidManifest.xml').read_text(), previous)
                STAGE.stage(tree, INPUTS, CONTRACT, report)
                STAGE.stage(tree, INPUTS, CONTRACT, report, verify=True)
                timestamps = {str(path): path.stat().st_mtime_ns for path in controller.rglob('*') if path.is_file()}
                STAGE.stage(tree, INPUTS, CONTRACT, report)
                self.assertEqual(timestamps, {str(path): path.stat().st_mtime_ns for path in controller.rglob('*') if path.is_file()})
                extra = controller / 'NativePrivate.java'
                extra.write_text('// keep unrelated work')
                with self.assertRaisesRegex(RuntimeError, 'unrelated'):
                    STAGE.stage(tree, INPUTS, CONTRACT, report)
                self.assertEqual(extra.read_text(), '// keep unrelated work')
                extra.unlink()
                source = controller / 'src/com/android/permissioncontroller/octosense/RoleSettingsBackend.java'
                source.write_text(source.read_text() + '\n// unrelated edit\n')
                with self.assertRaisesRegex(RuntimeError, 'Preserve modified'):
                    STAGE.stage(tree, INPUTS, CONTRACT, report)
                STAGE.REVISION = '0' * 40
                with self.assertRaisesRegex(RuntimeError, 'revision differs'):
                    STAGE.stage(tree, INPUTS, CONTRACT, report, check=True)
            finally:
                STAGE.REVISION = revision
