#!/usr/bin/env python3
"""Bind unsigned type-2 AppImages to their final kernel bytes, without executing them.

linuxdeploy adds RUNPATH to the kernel after package.py stages its receipt.
Only that proven ELF transformation is admitted here. The original staged/DEB
receipt is never changed. The caller must scan the resulting package normally.
"""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
KERNEL = 'usr/bin/octos-kernel'
RECEIPT = 'usr/lib/octosense/octos-kernel.json'
RUNPATH = b'$ORIGIN/../lib\0'


def scanner():
    spec = importlib.util.spec_from_file_location('appimage_container', ROOT / 'tools/release-scan.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def elf_sections(data):
    """Read the ELF64 little-endian section records used by Linux releases."""
    require(data[:7] == b'\x7fELF\x02\x01\x01' and len(data) >= 64, 'unsupported kernel ELF')
    offset = int.from_bytes(data[40:48], 'little')
    size, count, names_index = struct.unpack_from('<HHH', data, 58)
    require(size == 64 and 0 < names_index < count and offset + size * count <= len(data),
            'invalid kernel ELF section table')
    records = [struct.unpack_from('<IIQQQQIIQQ', data, offset + i * size) for i in range(count)]
    names = records[names_index]
    require(names[4] + names[5] <= len(data), 'invalid ELF section names')
    strings = data[names[4]:names[4] + names[5]]
    sections = {}
    ordered = []
    for record in records:
        name_offset, kind, flags, address, start, length, link, info, align, entry = record
        require(name_offset < len(strings), 'invalid ELF section name')
        end = strings.find(b'\0', name_offset)
        require(end >= 0, 'unterminated ELF section name')
        name = strings[name_offset:end].decode('ascii')
        require(name not in sections, 'duplicate ELF section')
        require(kind == 8 or start + length <= len(data), 'truncated ELF section')
        sections[name] = {'bytes': b'' if kind == 8 else data[start:start + length],
                          'kind': kind, 'flags': flags, 'address': address, 'size': length,
                          'link': link, 'info': info, 'align': align, 'entry': entry}
        ordered.append(name)
    return sections, ordered


def symbol_identity(section, sections, ordered, dynamic_anchor):
    data = section['bytes']
    require(section['entry'] == 24 and len(data) % 24 == 0, 'unsupported kernel symbol table')
    symbols = []
    require(section['link'] < len(ordered), 'invalid ELF symbol strings')
    strings = sections[ordered[section['link']]]['bytes']
    for name, info, other, index, value, size in struct.iter_unpack('<IBBHQQ', data):
        require(name < len(strings), 'invalid ELF symbol name')
        end = strings.find(b'\0', name)
        require(end >= 0, 'unterminated ELF symbol name')
        identity = index
        if 0 < index < len(ordered):
            identity = ordered[index]
            # patchelf leaves this exact local, hidden, zero-size anchor at
            # its original absolute address in the non-loaded symbol table.
            if (identity == '.dynamic' and strings[name:end] == b'_DYNAMIC' and
                    info == 0 and other == 2 and size == 0 and value == dynamic_anchor):
                value = ('unchanged_dynamic_anchor', value)
            else:
                value -= sections[identity]['address']
        symbols.append((name, info, other, identity, value, size))
    return symbols


def verify_runpath_transform(original, packaged):
    """Refuse receipt repair if linuxdeploy changed executable content.

    Section order/addresses and symbol section indices can move. All section
    contents stay byte-identical except the exact RUNPATH/dynamic-string change
    and semantically equivalent .symtab relocations made by patchelf.
    """
    if original == packaged:
        return
    before, before_order = elf_sections(original)
    after, after_order = elf_sections(packaged)
    require(original[16:32] == packaged[16:32] and original[48:52] == packaged[48:52] and
            set(before) == set(after), 'kernel ELF identity changed')
    require('.dynstr' in before and '.dynamic' in before, 'kernel dynamic sections missing')
    for name in before:
        a, b = before[name], after[name]
        require(all(a[key] == b[key] for key in ('kind', 'flags', 'align', 'entry')), 'kernel section attributes changed')
        if name == '.symtab':
            anchor = before['.dynamic']['address']
            require(symbol_identity(a, before, before_order, anchor) == symbol_identity(b, after, after_order, anchor),
                    'kernel symbol identities changed')
        elif name not in ('.dynstr', '.dynamic'):
            require(a['bytes'] == b['bytes'] and a['size'] == b['size'], 'kernel section contents changed')
    old_strings, new_strings = before['.dynstr']['bytes'], after['.dynstr']['bytes']
    require(new_strings == old_strings + RUNPATH, 'unexpected kernel RUNPATH strings')
    dynamic = []
    for sections in (before, after):
        data = sections['.dynamic']['bytes']
        require(len(data) % 16 == 0, 'invalid kernel dynamic table')
        entries = [(tag, value) for tag, value in struct.iter_unpack('<qQ', data) if tag != 0]
        for tag in (5, 10, 29):
            require(sum(key == tag for key, _ in entries) <= 1, 'duplicate kernel dynamic control tag')
        dynamic.append(entries)
    a, b = dynamic
    a_values, b_values = dict(a), dict(b)
    require(29 not in a_values and 15 not in a_values and 15 not in b_values and b_values.get(29) == len(old_strings),
            'unexpected kernel RUNPATH entry')
    for sections, entries, strings in ((before, a_values, old_strings), (after, b_values, new_strings)):
        require(entries.get(5) == sections['.dynstr']['address'] and entries.get(10) == len(strings),
                'kernel dynamic strings are inconsistent')
    require([(k, v) for k, v in a if k not in (5, 10, 29)] ==
            [(k, v) for k, v in b if k not in (5, 10, 29)], 'kernel dynamic dependencies changed')


def updated_receipt(embedded, staged, original, packaged):
    require(staged.get('sha256') == sha(original), 'staged kernel does not match its receipt')
    require(embedded == staged or embedded == {**staged, 'sha256': sha(packaged)},
            'AppImage receipt is not the staged kernel receipt')
    verify_runpath_transform(original, packaged)
    return {**staged, 'sha256': sha(packaged)}


def run(command):
    return subprocess.run(command, check=True, capture_output=True, env={'PATH': os.defpath, 'LC_ALL': 'C'},
                          timeout=600).stdout


def root_owners(listing):
    owners = re.findall(rb'^[dl-][rwxstST-]{9}\s+(\d+)/(\d+)\s+', listing, re.M)
    require(owners and all(uid == b'0' and gid == b'0' for uid, gid in owners),
            'AppImage must have root-owned regular files, directories and links')


def inventory(root):
    """Preserve every file/link, permission and mtime; no devices or sockets."""
    result, hardlinks = {}, {}
    for path in [root, *sorted(root.rglob('*'))]:
        meta = path.lstat()
        item = {'mode': stat.S_IMODE(meta.st_mode), 'mtime': int(meta.st_mtime)}
        if path.is_symlink():
            item['link'] = os.readlink(path)
        elif path.is_file():
            item['sha256'] = sha(path.read_bytes())
            hardlinks.setdefault((meta.st_dev, meta.st_ino), []).append(path.relative_to(root).as_posix())
        elif not path.is_dir():
            raise RuntimeError('unsupported AppImage filesystem entry')
        result[path.relative_to(root).as_posix()] = item
    for group in hardlinks.values():
        if len(group) > 1:
            for name in group:
                result[name]['hardlinks'] = sorted(group)
    return result


def regular(root, relative):
    path = root / relative
    require(path.is_file() and not path.is_symlink() and root.resolve() in path.resolve().parents,
            'missing or non-contained AppImage kernel/receipt')
    return path


def finalize_appimage(path, staged, original):
    """Atomically repair a freshly packaged unsigned AppImage; return its binding.

    The input runtime and payload are never executed. Failures leave the input
    unchanged. Re-extraction verifies that only receipt bytes changed.
    """
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), 'AppImage must be a regular file')
    data = path.read_bytes()
    offset, end = scanner().appimage_filesystem(data)
    prefix_sections, _ = elf_sections(data[:offset])
    for name in ('.sha256_sig', '.sig_key'):
        require(not any(prefix_sections.get(name, {}).get('bytes', b'')), 'signed AppImage cannot be repaired')
    require(not any(data[end:]), 'unexpected AppImage trailer')
    superblock = data[offset:offset + 96]
    require(superblock[56:64] == b'\xff' * 8, 'AppImage xattrs are unsupported for receipt repair')
    compressors = {1: 'gzip', 2: 'lzma', 3: 'lzo', 4: 'xz', 5: 'lz4', 6: 'zstd'}
    compressor = compressors.get(int.from_bytes(superblock[20:22], 'little'))
    require(compressor is not None, 'unsupported AppImage compressor')
    extractor, builder = shutil.which('unsquashfs'), shutil.which('mksquashfs')
    require(extractor and builder, 'AppImage kernel receipt finalization requires squashfs-tools')
    with tempfile.TemporaryDirectory(prefix='.appimage-receipt-', dir=path.parent) as temp:
        temp = Path(temp)
        source = temp / 'input.AppImage'
        source.write_bytes(data)
        root_owners(run([extractor, '-lln', '-offset', str(offset), str(source)]))
        tree = temp / 'payload'
        extract = [extractor, '-strict-errors', '-no-progress', '-no-xattrs', '-processors', '2']
        run([*extract, '-offset', str(offset), '-dest', str(tree), str(source)])
        before = inventory(tree)
        receipt = regular(tree, RECEIPT)
        kernel = regular(tree, KERNEL).read_bytes()
        embedded = json.loads(receipt.read_text())
        corrected = updated_receipt(embedded, staged, original, kernel)
        expected = before
        if embedded != corrected:
            meta = receipt.stat()
            receipt.write_text(json.dumps(corrected, indent=2) + '\n')
            os.utime(receipt, ns=(meta.st_atime_ns, meta.st_mtime_ns))
            expected = inventory(tree)
            require({k: v for k, v in before.items() if k != RECEIPT} ==
                    {k: v for k, v in expected.items() if k != RECEIPT}, 'unexpected payload mutation')
            fs = temp / 'filesystem.squashfs'
            run([builder, str(tree), str(fs), '-noappend', '-no-progress', '-processors', '2',
                 '-all-root', '-no-xattrs', '-comp', compressor, '-b', str(int.from_bytes(superblock[12:16], 'little')),
                 '-mkfs-time', str(int.from_bytes(superblock[8:12], 'little'))])
            rebuilt = data[:offset] + fs.read_bytes() + data[end:]
            new_offset, _ = scanner().appimage_filesystem(rebuilt)
            require(new_offset == offset and rebuilt[:offset] == data[:offset], 'AppImage runtime changed')
            candidate = temp / 'final.AppImage'
            candidate.write_bytes(rebuilt)
            root_owners(run([extractor, '-lln', '-offset', str(offset), str(candidate)]))
            check = temp / 'verified'
            run([*extract, '-offset', str(offset), '-dest', str(check), str(candidate)])
            require(inventory(check) == expected, 'AppImage repack changed payload content or metadata')
            require(json.loads(regular(check, RECEIPT).read_text()) == corrected and
                    sha(regular(check, KERNEL).read_bytes()) == corrected['sha256'], 'final kernel receipt mismatch')
            candidate.chmod(stat.S_IMODE(path.stat().st_mode))
            require(path.read_bytes() == data, 'AppImage changed during finalization')
            os.replace(candidate, path)
    return {'file': path.name, 'format': 'appimage', 'sha256': sha(path.read_bytes()), 'kernel': corrected,
            'finalization': {'input_sha256': sha(data), 'runtime_prefix_sha256': sha(data[:offset]),
                             'receipt_updated': embedded != corrected, 'artifact_executed': False,
                             'payload_before_sha256': sha(json.dumps(before, sort_keys=True).encode()),
                             'payload_after_sha256': sha(json.dumps(expected, sort_keys=True).encode())}}


def deb_binding(path, staged):
    """Check actual DEB bytes, not the shared pre-packaging receipt."""
    payload = {}
    for name, data in scanner().ar_members(Path(path).read_bytes()):
        if name.startswith('data.tar'):
            with tarfile.open(fileobj=io.BytesIO(data)) as archive:
                for member in archive.getmembers():
                    relative = member.name.removeprefix('./')
                    if relative in (KERNEL, RECEIPT):
                        require(member.isfile() and relative not in payload, 'invalid DEB kernel payload')
                        payload[relative] = archive.extractfile(member).read()
    require(set(payload) == {KERNEL, RECEIPT}, 'DEB kernel or receipt missing')
    require(json.loads(payload[RECEIPT]) == staged and sha(payload[KERNEL]) == staged['sha256'],
            'DEB kernel does not match the staged receipt')
    return {'file': Path(path).name, 'format': 'deb', 'sha256': sha(Path(path).read_bytes()), 'kernel': dict(staged)}


def finalize_linux(directory, formats, staged, original):
    bindings = []
    for kind, glob in (('deb', '*.deb'), ('appimage', '*.AppImage')):
        if kind not in formats:
            continue
        paths = list(Path(directory).glob(glob))
        require(len(paths) == 1, 'expected exactly one current Linux package per requested format')
        bindings.append(deb_binding(paths[0], staged) if kind == 'deb' else
                        finalize_appimage(paths[0], staged, original))
    return bindings
