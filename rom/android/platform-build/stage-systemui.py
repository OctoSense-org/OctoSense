#!/usr/bin/env python3
"""Stage the reviewed OctoSense SystemUI fork against one pinned framework revision.

--check computes changes without mutation. --verify requires exact staged bytes.
Writes hold the shared build lock and reject unrelated framework changes.
"""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import re
import subprocess
import xml.etree.ElementTree as ET

REVISION = 'ff7620a38e54c5f7ec14a5b8ccc5be1ba41e2b1b'
PREFIX = 'packages/SystemUI/'


def once(text, before, after):
    if text.count(before) != 1:
        raise RuntimeError('Pinned source mismatch: ' + before[:100])
    return text.replace(before, after, 1)


def style(text, name, values):
    pattern = r'(<style name="' + re.escape(name) + r'"[^>]*>)(.*?)(</style>)'
    match = re.search(pattern, text, re.S)
    if not match:
        raise RuntimeError('Missing pinned style: ' + name)
    body = match[2]
    for key, value in values.items():
        item = r'(<item name="' + re.escape(key) + r'">).*?(</item>)'
        if re.search(item, body):
            body, count = re.subn(item, lambda m: m[1] + value + m[2], body)
            assert count == 1
        else:
            body += '\n        <item name="' + key + '">' + value + '</item>\n    '
    return text[:match.start()] + match[1] + body + match[3] + text[match.end():]


def generate(read, inputs):
    changes = {}
    manifest = read('AndroidManifest.xml')
    manifest = once(manifest, 'android:label="@string/app_label"', 'android:label="@string/octosense_system_label"')
    manifest = once(manifest, '        <!-- Keep theme in sync', '''        <meta-data android:name="dev.makepad.octosense.SYSTEM_INTERFACE" android:value="1" />
        <activity android:name=".octosense.OctoSenseSystemActivity"
            android:exported="false" android:excludeFromRecents="true"
            android:showWhenLocked="false" android:theme="@style/Theme.OctoSense.System" />
        <!-- Keep theme in sync''')
    changes['AndroidManifest.xml'] = manifest
    bp = read('Android.bp')
    app = re.search(r'android_app \{\n    name: "SystemUI",.*?\n\}', bp, re.S)
    if not app: raise RuntimeError('Pinned SystemUI app target missing')
    candidate = once(app[0], '    name: "SystemUI",', '    name: "OctoSenseSystemUI",\n    overrides: ["SystemUI"],')
    bp += '\n// OctoSense ROM integration target; retains the upstream target for its tests.\n' + candidate + '\n'
    # The package/process/shared UID and native services remain the platform ones.
    changes['Android.bp'] = bp
    qs = read('src/com/android/systemui/qs/dagger/QSModule.java')
    qs = once(qs, 'public interface QSModule { }', '''public interface QSModule {
    @dagger.Binds
    @dagger.multibindings.IntoMap
    @dagger.multibindings.StringKey(com.android.systemui.octosense.OctoSenseTile.TILE_SPEC)
    com.android.systemui.qs.tileimpl.QSTileImpl<?> bindOctoSenseTile(
            com.android.systemui.octosense.OctoSenseTile tile);
}''')
    changes['src/com/android/systemui/qs/dagger/QSModule.java'] = qs
    config = read('res/values/config.xml')
    for name in ['quick_settings_tiles_default', 'quick_settings_tiles_stock']:
        pattern = r'(<string name="' + name + r'"[^>]*>)(\s*)([^<]+)(</string>)'
        config, count = re.subn(pattern, lambda m: m[1] + m[2] + 'octosense,' + m[3] + m[4], config)
        assert count == 1
    changes['res/values/config.xml'] = config
    styles = read('res/values/styles.xml')
    styles = style(styles, 'Theme.SystemUI', {'android:colorAccent': '@color/octosense_accent'})
    palette = {'surfaceBright':'surface', 'scHigh':'card', 'primary':'accent', 'tertiary':'accent',
        'onSurface':'text', 'onSurfaceVariant':'muted', 'outline':'muted', 'shadeActive':'accent',
        'onShadeActive':'on_accent', 'onShadeActiveVariant':'on_accent', 'shadeInactive':'card',
        'onShadeInactive':'text', 'onShadeInactiveVariant':'muted', 'shadeDisabled':'disabled', 'underSurface':'surface'}
    styles = style(styles, 'Theme.SystemUI.QuickSettings', {key:'@color/octosense_' + color for key,color in palette.items()})
    styles = style(styles, 'Theme.SystemUI.Dialog', {'android:colorBackground':'@color/octosense_surface',
        'android:colorAccent':'@color/octosense_accent', 'android:buttonCornerRadius':'18dp'})
    styles = style(styles, 'TextAppearance.StatusBar.Clock', {'android:fontFamily':'sans-serif-medium', 'android:letterSpacing':'0.02'})
    changes['res/values/styles.xml'] = styles
    colors = read('res/values/colors.xml')
    for name, value in {'global_actions_lite_background':'#181624', 'global_actions_lite_button_background':'#2c2e3e',
        'global_actions_lite_button_background_focused':'#6750a4', 'global_actions_lite_text':'#f5f5fa'}.items():
        colors, count = re.subn(r'(<color name="' + name + r'">)[^<]*(</color>)',lambda m:m[1]+value+m[2],colors)
        assert count == 1
    changes['res/values/colors.xml'] = colors
    # Brand is part of the clock's moving container, retaining burn-in movement
    # and wallpaper-aware contrast. Native clock/media/accessibility IDs survive.
    keyguard = read('res-keyguard/layout/keyguard_status_view.xml')
    keyguard = once(keyguard, '        <include\n            layout="@layout/keyguard_clock_switch"', '''        <TextView
            android:layout_width="wrap_content" android:layout_height="wrap_content"
            android:paddingBottom="8dp" android:text="@string/octosense_brand"
            android:textSize="12sp" android:letterSpacing="0.16"
            android:textColor="?attr/wallpaperTextColor" android:fontFamily="sans-serif-medium" />
        <include
            layout="@layout/keyguard_clock_switch"''')
    changes['res-keyguard/layout/keyguard_status_view.xml'] = keyguard
    footer = read('res/layout/qs_footer_impl.xml')
    footer = once(footer, '            <TextView\n                android:id="@+id/build"', '''            <TextView
                android:layout_width="wrap_content" android:layout_height="match_parent"
                android:gravity="center_vertical" android:text="@string/octosense_brand"
                android:textSize="11sp" android:letterSpacing="0.12"
                android:textColor="?attr/onSurfaceVariant" android:paddingEnd="12dp" />
            <TextView
                android:id="@+id/build"''')
    changes['res/layout/qs_footer_impl.xml'] = footer
    # Keep native button IDs, hit areas, long presses, RTL and contrast handling.
    for name, path in {
        'ic_sysbar_home': 'M7,2 L13,2 L18,7 L18,13 L13,18 L7,18 L2,13 L2,7 Z M8,4 L4,8 L4,12 L8,16 L12,16 L16,12 L16,8 L12,4 Z',
        'ic_sysbar_recent': 'M3,3 L12,3 L12,5 L5,5 L5,14 L3,14 Z M7,7 L17,7 L17,17 L7,17 Z M9,9 L9,15 L15,15 L15,9 Z',
    }.items():
        original = read('res/drawable/' + name + '.xml')
        original, count = re.subn(r'android:pathData="[^"]+"', 'android:fillType="evenOdd" android:pathData="' + path + '"', original)
        assert count == 1
        changes['res/drawable/' + name + '.xml'] = original
    for file in sorted((inputs / 'files').rglob('*')):
        if file.is_file():
            relative = str(file.relative_to(inputs / 'files'))
            assert relative not in changes
            changes[relative] = file.read_text()
    for path, text in changes.items():
        if path.endswith('.xml'): ET.fromstring(text)
    return changes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tree', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--check', action='store_true'); mode.add_argument('--verify', action='store_true')
    args = parser.parse_args()
    repo = args.tree.resolve() / 'frameworks/base'
    inputs = Path(__file__).resolve().parent / 'systemui'
    def git(*parts): return subprocess.check_output(['git','-C',str(repo),*parts], text=True)
    if git('rev-parse', 'HEAD').strip() != REVISION: raise RuntimeError('Framework revision differs')
    changes = generate(lambda p: git('show', 'HEAD:' + PREFIX + p), inputs)
    original = {}
    for path in changes:
        result = subprocess.run(['git','-C',str(repo),'show','HEAD:' + PREFIX + path], capture_output=True, text=True)
        original[path] = result.stdout if result.returncode == 0 else None
    dirty = set(git('diff', 'HEAD', '--name-only').splitlines()) | set(git('ls-files', '--others', '--exclude-standard').splitlines())
    if not dirty <= {PREFIX + p for p in changes}: raise RuntimeError('Unrelated framework changes: ' + repr(dirty))
    for path, expected in changes.items():
        file = repo / PREFIX / path
        actual = file.read_text() if file.exists() else None
        if args.verify and actual != expected: raise RuntimeError('Staged file differs: ' + path)
        if not args.verify and actual not in (original[path], expected): raise RuntimeError('Preserve modified source: ' + path)
    if not args.check and not args.verify:
        with (args.tree / '.octosense-build.lock').open('a') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            for path, value in changes.items():
                file = repo / PREFIX / path; file.parent.mkdir(parents=True, exist_ok=True)
                if not file.exists() or file.read_text() != value: file.write_text(value)
    report = {'status':'pass','mode':'check' if args.check else 'verify' if args.verify else 'stage',
        'framework_revision':REVISION,'target':'OctoSenseSystemUI','package':'com.android.systemui',
        'files': {PREFIX + p: hashlib.sha256(value.encode()).hexdigest() for p,value in changes.items()},
        'original_files': {PREFIX + p: hashlib.sha256(value.encode()).hexdigest() if value is not None else None for p,value in original.items()},
        'device_deployed':False}
    args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'status':report['status'],'mode':report['mode'],'files':len(changes),'target':report['target']}))


if __name__ == '__main__': main()
