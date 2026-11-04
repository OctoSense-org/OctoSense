#!/usr/bin/env python3
"""Admin-authorized catalog publication. Never executes candidate source code."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request

REPOSITORY = 'OctoSense-org/OctoSense-App-Hub'
REPOSITORY_ID = 1378439410
OWNER_ID = 328148893
SUBJECT = 'catalog-v2.payload.json'
MAX_TREE_BYTES = 256 * 1024 * 1024
MAX_FILE_BYTES = 64 * 1024 * 1024
MAX_FILES = 20000


class Refused(Exception):
    pass


def require(value, message):
    if not value:
        raise Refused(message)


def digest(path):
    result = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            result.update(block)
    return result.hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def command(args, cwd=None, env=None):
    result = subprocess.run(args, cwd=cwd, env=env, capture_output=True)
    require(result.returncode == 0, 'Trusted command failed; publication stopped')
    return result.stdout


def api(path):
    token = os.environ.get('GH_TOKEN', '')
    require(token, 'GitHub job authentication is unavailable')
    request = urllib.request.Request('https://api.github.com/' + path, headers={
        'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'octosense-catalog-publisher'})
    # Refuse redirects so job credentials never follow a changed API origin.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs):
            return None
    try:
        with urllib.request.build_opener(NoRedirect).open(request, timeout=30) as response:
            data = response.read(2 * 1024 * 1024 + 1)
        require(len(data) <= 2 * 1024 * 1024, 'GitHub response exceeds the limit')
        return json.loads(data)
    except (urllib.error.URLError, ValueError):
        raise Refused('GitHub authorization lookup failed; publication stopped') from None


def authorize(env=None, fetch=api):
    env = os.environ if env is None else env
    require(env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch', 'Only explicit admin dispatch may sign')
    require(env.get('GITHUB_REF') == 'refs/heads/main', 'Only the trusted main workflow may sign')
    require(env.get('GITHUB_REPOSITORY') == REPOSITORY, 'Unexpected repository')
    repository = fetch('repos/' + REPOSITORY)
    require(repository.get('id') == REPOSITORY_ID
            and repository.get('owner', {}).get('id') == OWNER_ID
            and repository.get('private') is False, 'Repository identity or visibility changed')
    for actor in {env.get('GITHUB_ACTOR', ''), env.get('GITHUB_TRIGGERING_ACTOR', '')}:
        require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]{0,38}', actor), 'Missing authenticated actor')
        permission = fetch(f'repos/{REPOSITORY}/collaborators/{actor}/permission')
        require(permission.get('permission') == 'admin', 'Both the original and rerun actors must be admins')
        if actor == env.get('GITHUB_ACTOR'):
            require(str(permission.get('user', {}).get('id')) == env.get('GITHUB_ACTOR_ID'),
                    'The authenticated actor identity changed')


def safe_path(value):
    require(isinstance(value, str) and value and '\\' not in value
            and not any(ord(c) < 32 for c in value), 'Invalid publication path')
    path = PurePosixPath(value)
    require(not path.is_absolute() and value == path.as_posix()
            and all(part not in ('..', '.', '.git') for part in path.parts), 'Noncanonical publication path')
    return path


def regular_file(root, relative):
    path = safe_path(relative)
    current = root
    require(not current.is_symlink(), 'Publication root is a link')
    for part in path.parts:
        current = current / part
        require(not current.is_symlink(), 'Publication path contains a link')
    require(current.is_file(), 'Publication file is missing or special')
    return current


def inputs(commit, candidate, approved):
    require(all(isinstance(value, str) for value in (commit, candidate, approved)), 'Missing publication inputs')
    require(re.fullmatch('[0-9a-f]{40}', commit), 'Use a complete lowercase candidate commit')
    require(re.fullmatch('[a-z0-9][a-z0-9-]{0,63}', candidate), 'Invalid candidate name')
    require(re.fullmatch('[0-9a-f]{64}', approved), 'Approve the exact prepared SHA-256')


def extract_candidate(repo, commit, name, destination):
    prefix = 'catalog-candidates/' + name
    destination.mkdir(mode=0o700, parents=True, exist_ok=False)
    process = subprocess.Popen(['git', 'archive', '--format=tar', commit, prefix], cwd=repo,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    total = 0
    seen = set()
    try:
        with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
            for member in archive:
                original = member.name.rstrip('/') if member.isdir() else member.name
                path = safe_path(original)
                # git archive emits ancestor directories as well.
                if member.isdir() and original in ('catalog-candidates', prefix):
                    continue
                require(original.startswith(prefix + '/'), 'Archive escapes candidate directory')
                relative = safe_path(original[len(prefix) + 1:])
                require(relative.as_posix() not in seen, 'Repeated candidate path')
                seen.add(relative.as_posix())
                require(len(seen) <= MAX_FILES, 'Too many candidate files')
                target = destination.joinpath(*relative.parts)
                require(member.isdir() or member.isfile(), 'Links and special files are refused')
                if member.isdir():
                    target.mkdir(mode=0o700, parents=True, exist_ok=True)
                    continue
                total += member.size
                require(0 <= member.size <= MAX_FILE_BYTES and total <= MAX_TREE_BYTES, 'Candidate exceeds size limits')
                target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                with archive.extractfile(member) as source, target.open('xb') as output:
                    shutil.copyfileobj(source, output)
        require(process.wait(timeout=30) == 0, 'Cannot read the reviewed candidate')
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        process.stdout.close()
        process.stderr.close()


def baseline(repo):
    name = 'catalog-v2.json' if (repo / 'catalog-v2.json').exists() else 'catalog.json'
    path = repo / name
    require(path.is_file() and not path.is_symlink(), 'The trusted base catalog is unavailable')
    return path


def listed_files(root, native):
    files = []
    for item in native['added_artifacts']:
        # A new app version adds its bundle, pack and index; a new shared
        # component version (App Hub ADR 0003) its .wasm file and index.
        require(set(item) in ({'bundle', 'pack', 'index'}, {'component', 'index'}), 'Unexpected admission artifact receipt')
        for field, value in item.items():
            path = safe_path(value)
            require(path.parts[0] == ('index' if field == 'index' else 'artifacts'), 'Unexpected artifact destination')
            require(field != 'component' or path.suffix == '.wasm', 'Unexpected component artifact')
            source = root.joinpath(*path.parts)
            require(not source.is_symlink(), 'Artifact link refused')
            if field == 'bundle':
                require(source.is_dir(), 'Admitted bundle missing')
                for child in sorted(source.rglob('*')):
                    require(not child.is_symlink(), 'Artifact link refused')
                    if child.is_dir():
                        continue
                    require(child.is_file(), 'Special artifact refused')
                    files.append(child.relative_to(root).as_posix())
            else:
                require(source.is_file(), 'Admitted pack or index missing')
                files.append(path.as_posix())
    require(len(files) == len(set(files)), 'Duplicate publication file')
    return sorted(files)


def prepare(repo, hub, commit, name, approved, out):
    inputs(commit, name, approved)
    require(command(['git', 'rev-parse', 'HEAD'], repo).decode().strip() == os.environ['GITHUB_SHA'],
            'Trusted tooling checkout differs from workflow revision')
    command(['git', 'merge-base', '--is-ancestor', commit, os.environ['GITHUB_SHA']], repo)
    require(not command(['git', 'status', '--porcelain', '--untracked-files=no'], repo), 'Trusted checkout is dirty')
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    candidate = out / 'candidate'
    extract_candidate(repo, commit, name, candidate)
    base = baseline(repo)
    payload = out / SUBJECT
    raw = command([str(hub), 'catalog-prepare', '--base', str(base), '--candidate', str(candidate/'catalog.json'),
                   '--artifact-root', str(candidate), '--out', str(payload)], repo)
    native = json.loads(raw)
    require(native['subject'] == SUBJECT, 'Unexpected attestation subject')
    require(native['payload_sha256'] == approved == digest(payload), 'Prepared catalog differs from admin-approved digest')
    require(native['base_sha256'] == digest(base), 'Base catalog changed during validation')
    files = listed_files(candidate, native)
    hashes = {}
    for relative in files:
        source = candidate / relative
        target = out / 'publication' / relative
        target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        hashes[relative] = digest(target)
        require(not (repo / relative).exists(), 'Publication would replace an existing artifact')
    receipt = {'schema': 1, 'repository_id': REPOSITORY_ID, 'owner_id': OWNER_ID,
               'workflow_commit': os.environ['GITHUB_SHA'], 'candidate_commit': commit, 'candidate': name,
               'base_catalog': base.name, 'base_sha256': digest(base), 'approved_sha256': approved,
               'native': native, 'files': hashes}
    (out/'receipt.json').write_bytes(canonical(receipt)+b'\n')
    shutil.rmtree(candidate)
    return receipt


def recheck(repo, packet, approved, fetch=api):
    require(command(['git', 'rev-parse', 'HEAD'], repo).decode().strip() == os.environ['GITHUB_SHA'],
            'Trusted tooling checkout differs from workflow revision')
    require(not command(['git', 'status', '--porcelain', '--untracked-files=no'], repo), 'Trusted checkout is dirty')
    receipt = json.loads((packet/'receipt.json').read_bytes())
    require(receipt['schema'] == 1 and receipt['repository_id'] == REPOSITORY_ID
            and receipt['owner_id'] == OWNER_ID, 'Invalid prepared receipt identity')
    require(receipt['workflow_commit'] == os.environ['GITHUB_SHA'], 'Prepared job came from another workflow revision')
    require(receipt['approved_sha256'] == approved == digest(packet/SUBJECT), 'Prepared payload was changed')
    require(baseline(repo).name == receipt['base_catalog']
            and digest(baseline(repo)) == receipt['base_sha256'], 'The base catalog changed')
    current = fetch('repos/'+REPOSITORY+'/git/ref/heads/main')
    require(current.get('object', {}).get('sha') == receipt['workflow_commit'], 'Main advanced; review and prepare again')
    for relative, expected in receipt['files'].items():
        require(safe_path(relative).parts[0] in ('artifacts', 'index'), 'Unexpected publication destination')
        path = regular_file(packet/'publication', relative)
        require(digest(path) == expected, 'Prepared artifact was changed')
        require(not (repo/relative).exists(), 'An immutable artifact already exists')
    actual = {p.relative_to(packet/'publication').as_posix()
              for p in (packet/'publication').rglob('*') if p.is_file()}
    require(actual == set(receipt['files']), 'Unlisted publication artifact')
    return receipt


def publish(repo, hub, packet, proof, approved, dry_run):
    authorize()
    receipt = recheck(repo, packet, approved)
    envelope = packet/'catalog-v2.json'
    command([str(hub), 'catalog-envelope', '--catalog', str(packet/SUBJECT),
             '--attestation', str(proof), '--out', str(envelope)], repo)
    command([str(hub), 'catalog-verify', str(envelope)], repo)
    if dry_run:
        return {'signed': True, 'published': False, 'sequence': receipt['native']['sequence']}
    # Recheck after proof verification, immediately before staging the transaction.
    authorize()
    recheck(repo, packet, approved)
    allowed = sorted([*receipt['files'], 'catalog-v2.json'])
    for relative in receipt['files']:
        target = repo/relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(packet/'publication'/relative, target)
    shutil.copyfile(envelope, repo/'catalog-v2.json')
    command(['git', 'add', '--', *allowed], repo)
    changed = command(['git', 'diff', '--cached', '--name-only', '-z'], repo).decode().split('\0')
    require(sorted(p for p in changed if p) == allowed, 'Unexpected staged publication changes')
    command(['git', '-c', 'core.hooksPath=/dev/null', '-c', 'user.name=github-actions[bot]',
             '-c', 'user.email=41898282+github-actions[bot]@users.noreply.github.com', 'commit',
             '-m', f"Publish GitHub-attested catalog {receipt['native']['sequence']}"], repo)
    env = os.environ.copy()
    # Scope the job token to one Git operation, never a file, URL or argv value.
    authorization = base64.b64encode(('x-access-token:'+env['GH_TOKEN']).encode()).decode()
    env.update(GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='http.https://github.com/.extraheader',
               GIT_CONFIG_VALUE_0='AUTHORIZATION: basic '+authorization, GIT_TERMINAL_PROMPT='0')
    # The commit's parent is the checked main SHA; a concurrent main update
    # causes this non-force push to fail. No retry can change its parent.
    command(['git', '-c', 'core.hooksPath=/dev/null', 'push', '--porcelain', 'origin', 'HEAD:refs/heads/main'], repo, env)
    return {'signed': True, 'published': True, 'sequence': receipt['native']['sequence']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['authorize', 'prepare', 'recheck', 'publish'])
    parser.add_argument('--repo', type=Path, default=Path.cwd())
    parser.add_argument('--hub', type=Path)
    parser.add_argument('--packet', type=Path)
    parser.add_argument('--proof', type=Path)
    parser.add_argument('--dry-run', action='store_true')
    args = parser.parse_args()
    try:
        args.repo = args.repo.resolve()
        require(args.operation == 'authorize' or args.packet is not None, 'This operation requires --packet')
        require(args.operation not in ('prepare', 'publish') or args.hub is not None, 'This operation requires --hub')
        require(args.operation != 'publish' or args.proof is not None, 'Publishing requires --proof')
        if args.hub: args.hub = args.hub.resolve()
        if args.packet: args.packet = args.packet.resolve()
        approved = os.environ.get('APPROVED_CATALOG_SHA256', '')
        if args.operation == 'authorize':
            authorize()
            result = {'admin_authorized': True}
        elif args.operation == 'prepare':
            authorize()
            result = prepare(args.repo, args.hub, os.environ['CANDIDATE_COMMIT'],
                             os.environ['CANDIDATE_NAME'], approved, args.packet)
        elif args.operation == 'recheck':
            authorize()
            result = recheck(args.repo, args.packet, approved)
        else:
            result = publish(args.repo, args.hub, args.packet, args.proof, approved, args.dry_run)
        print(json.dumps(result, sort_keys=True))
    except (Refused, KeyError, ValueError, OSError, tarfile.TarError, subprocess.SubprocessError) as error:
        # Never print HTTP headers, commands, token values or candidate bytes.
        print('catalog-publish: '+(str(error) if isinstance(error, Refused) else type(error).__name__), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
