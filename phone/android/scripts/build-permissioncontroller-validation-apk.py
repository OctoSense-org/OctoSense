#!/usr/bin/env python3
"""Add the finite Settings adapters to an exact emulator PermissionController APK.

Validation only: retains every original DEX/resource payload, adds adapter-only
DEX and the staged service/operation pairs, then uses the caller's emulator test
key. Does not deploy, modify APEX files, or build a production ROM artifact.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import xml.etree.ElementTree as ET
import zipfile

ANDROID = 'http://schemas.android.com/apk/res/android'
NONE = 0xffffffff
ROOT = Path(__file__).resolve().parents[3]
PREFIX = 'com.android.permissioncontroller.octosense.'
ROLE_COMPONENTS = {'OctoSenseRolesService': 'service', 'OctoSenseRoleConfirmationActivity': 'activity'}
PERMISSION_COMPONENTS = {'OctoSensePermissionsService': 'service', 'OctoSensePermissionOperationActivity': 'activity'}


def checked_components(path):
    root = ET.parse(path).getroot()
    names = [node.get('{' + ANDROID + '}name') for node in root]
    assert len(names) == len(set(names)), 'Duplicate adapter component'
    expected = {PREFIX + name: kind for name, kind in ROLE_COMPONENTS.items()}
    permission_names = {PREFIX + name for name in PERMISSION_COMPONENTS}
    if permission_names.intersection(names):
        expected.update({PREFIX + name: kind for name, kind in PERMISSION_COMPONENTS.items()})
    assert set(names) == set(expected), 'Unexpected or incomplete adapter components'
    for node, name in zip(root, names):
        assert node.tag == expected[name] and len(node) == 0
        if node.tag == 'service':
            assert node.get('{' + ANDROID + '}exported') == 'true'
            assert node.get('{' + ANDROID + '}permission') == 'dev.makepad.octosense.permission.BIND_AGENT_PLATFORM'
        else:
            assert node.get('{' + ANDROID + '}exported') == 'false'
    return root


def chunks(data, start=8):
    result = []
    while start < len(data):
        kind, header, size = struct.unpack_from('<HHI', data, start)
        assert size >= header >= 8 and start + size <= len(data)
        result.append((kind, data[start:start + size]))
        start += size
    assert start == len(data)
    return result


def length(data, offset, utf8):
    fmt, width, bit = ('B', 1, 0x80) if utf8 else ('H', 2, 0x8000)
    first = struct.unpack_from('<' + fmt, data, offset)[0]
    if first & bit:
        second = struct.unpack_from('<' + fmt, data, offset + width)[0]
        return ((first & (bit - 1)) * bit * 2 + second, offset + width * 2)
    return first, offset + width


class Strings:
    def __init__(self, data):
        kind, header, size, count, styles, flags, start, style_start = struct.unpack_from('<HH6I', data)
        assert kind == 1 and header == 28 and size == len(data) and styles == style_start == 0
        self.flags, self.utf8 = flags & ~1, bool(flags & 0x100)
        self.offsets = list(struct.unpack_from('<' + 'I' * count, data, 28))
        self.data = bytearray(data[start:])
        self.values = []
        for offset in self.offsets:
            size, position = length(self.data, offset, self.utf8)
            if self.utf8:
                size, position = length(self.data, position, True)
            self.values.append(bytes(self.data[position:position + size * (1 if self.utf8 else 2)]).decode('utf-8' if self.utf8 else 'utf-16-le'))

    def add(self, value, force=False):
        if not force and value in self.values:
            return self.values.index(value)
        encoded = value.encode('utf-8' if self.utf8 else 'utf-16-le')
        units = len(value.encode('utf-16-le')) // 2
        assert (units < 128 and len(encoded) < 128) if self.utf8 else units < 0x8000
        self.offsets.append(len(self.data))
        self.data.extend((bytes([units, len(encoded)]) if self.utf8 else struct.pack('<H', units)) + encoded + (b'\0' if self.utf8 else b'\0\0'))
        self.values.append(value)
        return len(self.values) - 1

    def encode(self):
        data = bytes(self.data)
        data += b'\0' * (-len(data) % 4)
        start = 28 + 4 * len(self.offsets)
        return struct.pack('<HH6I', 1, 28, start + len(data), len(self.offsets), 0, self.flags, start, 0) + struct.pack('<' + 'I' * len(self.offsets), *self.offsets) + data


def patch_manifest(original, components, resources, attrs):
    all_chunks = chunks(original)
    assert [kind for kind, _ in all_chunks[:2]] == [1, 0x180]
    strings = Strings(all_chunks[0][1])
    old_values = list(strings.values)
    resource_ids = list(struct.unpack('<' + 'I' * ((len(all_chunks[1][1]) - 8) // 4), all_chunks[1][1][8:]))
    old_ids = list(resource_ids)
    ns = strings.add(ANDROID)

    def attribute(name, value):
        rid = attrs[name]
        if rid in resource_ids:
            index = resource_ids.index(rid)
            assert strings.values[index] == name
        else:
            index = strings.add(name, force=True)
            resource_ids.extend([0] * (index + 1 - len(resource_ids)))
            resource_ids[index] = rid
        if value in ('true', 'false'):
            raw, kind, data = NONE, 0x12, NONE if value == 'true' else 0
        elif value.startswith('@'):
            raw, kind, data = NONE, 1, resources[value[1:]]
        else:
            raw = data = strings.add(value)
            kind = 3
        return rid, struct.pack('<IIIHBBI', ns, index, raw, 8, 0, kind, data)

    additions = bytearray()
    root = checked_components(components)
    for node in root:
        assert len(node) == 0
        tag = strings.add(node.tag)
        encoded = []
        for key, value in node.attrib.items():
            assert key.startswith('{' + ANDROID + '}')
            encoded.append(attribute(key.split('}', 1)[1], value))
        body = struct.pack('<II6H', NONE, tag, 20, 20, len(encoded), 0, 0, 0) + b''.join(value for _, value in sorted(encoded))
        additions.extend(struct.pack('<HHIII', 0x102, 16, 16 + len(body), 0, NONE) + body)
        additions.extend(struct.pack('<HHIIIII', 0x103, 16, 24, 0, NONE, NONE, tag))
    assert strings.values[:len(old_values)] == old_values and resource_ids[:len(old_ids)] == old_ids
    pool = strings.encode()
    mapping = struct.pack('<HHI', 0x180, 8, 8 + len(resource_ids) * 4) + struct.pack('<' + 'I' * len(resource_ids), *resource_ids)
    rest, stack, inserted = bytearray(), [], False
    for kind, chunk in all_chunks[2:]:
        if kind in (0x102, 0x103):
            tag = strings.values[struct.unpack_from('<I', chunk, 20)[0]]
            if kind == 0x102:
                stack.append(tag)
            else:
                assert stack.pop() == tag
                if tag == 'application':
                    assert stack == ['manifest'] and not inserted
                    rest.extend(additions)
                    inserted = True
        rest.extend(chunk)
    assert inserted and not stack
    # Every pre-existing XML node is retained byte-for-byte with identical indices.
    assert bytes(rest).replace(bytes(additions), b'', 1) == b''.join(chunk for _, chunk in all_chunks[2:])
    body = pool + mapping + rest
    return struct.pack('<HHI', 3, 8, len(body) + 8) + body


def dex_classes(data):
    assert data.startswith(b'dex\n') and struct.unpack_from('<I', data, 32)[0] == len(data)
    string_count, string_off, type_count, type_off = struct.unpack_from('<4I', data, 56)
    class_count, class_off = struct.unpack_from('<2I', data, 96)
    strings = []
    for offset in struct.unpack_from('<' + 'I' * string_count, data, string_off):
        while data[offset] & 0x80:
            offset += 1
        offset += 1
        strings.append(data[offset:data.index(0, offset)].decode('utf-8', errors='replace'))
    types = struct.unpack_from('<' + 'I' * type_count, data, type_off)
    return {strings[types[struct.unpack_from('<I', data, class_off + i * 32)[0]]] for i in range(class_count)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('original', 'adapter_dex', 'output', 'sdk', 'java_home', 'test_key', 'test_cert'):
        parser.add_argument('--' + name.replace('_', '-'), type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'Refusing to overwrite a validation artifact'
    args.output.mkdir(parents=True)
    build = args.sdk / 'build-tools/35.0.0'
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*parts):
        return subprocess.check_output([str(p) for p in parts], env=env, text=True)

    cert = run(build / 'apksigner', 'verify', '--print-certs', args.original)
    # This tool is exclusively for an emulator signed with the public AOSP test key.
    assert 'c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8' in cert
    badging = run(build / 'aapt2', 'dump', 'badging', args.original)
    assert re.search(r"^package: name='com.android.permissioncontroller' ", badging, re.M), 'Wrong original package'
    resources = {name: int(value, 16) for value, name in re.findall(r'resource (0x[0-9a-f]+) (\S+)', run(build / 'aapt2', 'dump', 'resources', args.original))}
    attrs = {name: int(value) for name, value in re.findall(r'int (\w+) = (\d+);', run(args.java_home / 'bin/javap', '-constants', '-classpath', args.sdk / 'platforms/android-35/android.jar', 'android.R$attr'))}
    adapter = args.adapter_dex.read_bytes()
    added = dex_classes(adapter)
    assert added and all(name.startswith(('Lcom/android/permissioncontroller/octosense/', 'Ldev/makepad/octosense/roles/', 'Ldev/makepad/octosense/permissions/')) for name in added), 'Adapter DEX includes native or stub classes'
    components = ROOT / 'home/android/platform-build/permissioncontroller/components.xml'
    component_names = [node.get('{' + ANDROID + '}name') for node in checked_components(components)]
    assert all('L' + name.replace('.', '/') + ';' in added for name in component_names), 'Missing adapter component class'
    preserved, native_classes = {}, set()
    with zipfile.ZipFile(args.original) as source, zipfile.ZipFile(args.output / 'unsigned.apk', 'w') as target:
        original_dex = [n for n in source.namelist() if re.fullmatch(r'classes(?:\d+)?\.dex', n)]
        next_dex = 'classes' + str(len(original_dex) + 1) + '.dex'
        assert next_dex not in source.namelist()
        for entry in source.infolist():
            if re.fullmatch(r'META-INF/(?:MANIFEST\.MF|[^/]+\.(?:SF|RSA|DSA|EC))', entry.filename, re.IGNORECASE):
                continue
            data = source.read(entry)
            if entry.filename in original_dex:
                native_classes.update(dex_classes(data))
            if entry.filename == 'AndroidManifest.xml':
                data = patch_manifest(data, components, resources, attrs)
            else:
                preserved[entry.filename] = hashlib.sha256(data).hexdigest()
            target.writestr(entry, data)
        assert not native_classes.intersection(added), 'Adapter duplicates an existing class'
        target.writestr(next_dex, adapter, compress_type=zipfile.ZIP_DEFLATED)
    run(build / 'zipalign', '-f', '4', args.output / 'unsigned.apk', args.output / 'aligned.apk')
    apk = args.output / 'PermissionController-role-validation.apk'
    run(build / 'apksigner', 'sign', '--key', args.test_key, '--cert', args.test_cert, '--out', apk, args.output / 'aligned.apk')
    signed_cert = run(build / 'apksigner', 'verify', '--print-certs', apk)
    assert 'c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8' in signed_cert
    with zipfile.ZipFile(apk) as final:
        assert all(hashlib.sha256(final.read(name)).hexdigest() == value for name, value in preserved.items())
        assert final.read(next_dex) == adapter
    (args.output / 'manifest.txt').write_text(run(build / 'aapt2', 'dump', 'xmltree', apk, '--file', 'AndroidManifest.xml'))
    report = {'validation_only': True, 'source_sha256': hashlib.sha256(args.original.read_bytes()).hexdigest(),
              'apk_sha256': hashlib.sha256(apk.read_bytes()).hexdigest(), 'adapter_sha256': hashlib.sha256(adapter).hexdigest(),
              'native_class_count': len(native_classes), 'adapter_components': component_names,
              'adapter_classes': sorted(added), 'preserved_payloads': preserved}
    (args.output / 'identity-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: value for key, value in report.items() if key not in ('adapter_classes', 'preserved_payloads')}, indent=2))
    print('Preserved payloads:', len(preserved), 'Adapter classes:', len(added), 'APK:', apk)


if __name__ == '__main__':
    main()
