#!/usr/bin/env python3
"""Generate/check Home's Binder client with the pinned Android AIDL compiler."""
import argparse
import difflib
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path('vendor/octosense/agent/src/dev/makepad/octosense/agent/IAgentPlatform.aidl')
TARGET = ROOT / 'home/resources/android/java/dev/makepad/octosense/agent/IAgentPlatform.java'


def generate(sdk):
    aidl = sdk / 'build-tools/35.0.0/aidl'
    framework = sdk / 'platforms/android-35/framework.aidl'
    if not aidl.is_file() or not framework.is_file():
        raise RuntimeError('Android SDK build-tools 35.0.0 and platform android-35 are required')
    with tempfile.TemporaryDirectory() as directory:
        output = Path(directory) / 'IAgentPlatform.java'
        subprocess.run([str(aidl), '-p' + str(framework), str(SOURCE), str(output)],
                       cwd=ROOT, check=True)
        # The compiler embeds its invocation, including private SDK/output paths.
        return re.sub(r'^ \* Using:.*\n', '', output.read_text(), flags=re.MULTILINE)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, default=os.environ.get('ANDROID_SDK_ROOT') or os.environ.get('ANDROID_HOME'))
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.sdk is None:
        parser.error('Pass --sdk or set ANDROID_SDK_ROOT/ANDROID_HOME')
    expected = generate(args.sdk.resolve())
    if args.check:
        actual = TARGET.read_text()
        if actual != expected:
            print(''.join(difflib.unified_diff(actual.splitlines(True), expected.splitlines(True),
                                             fromfile=str(TARGET.relative_to(ROOT)), tofile='generated')), end='')
            raise SystemExit('Binder client differs; run scripts/generate-agent-aidl.py')
        print('Agent Binder client matches the AIDL source')
    else:
        TARGET.write_text(expected)


if __name__ == '__main__':
    main()
