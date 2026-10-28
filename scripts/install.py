#!/usr/bin/env python3
"""Stage reviewed release files; install-files never starts services or activates plugins."""
import argparse
import hashlib
import json
import os
import platform
import shutil
import stat
import tarfile
import tempfile
from pathlib import Path
import re
import subprocess
import sys


class InstallerError(Exception):
    pass


def run(command, cwd=None, env=None, label='command', timeout=1200):
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise InstallerError(label + ' failed; no service or plugin was activated')
    return result.stdout.strip()


def reviewed_source(source, revision):
    source = source.resolve(strict=True)
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise InstallerError('a full reviewed Git commit is required')
    if Path(run(['git', 'rev-parse', '--show-toplevel'], source)).resolve() != source:
        raise InstallerError('source must be a Git checkout root')
    if run(['git', 'rev-parse', 'HEAD'], source) != revision:
        raise InstallerError('checkout differs from reviewed commit')
    if run(['git', 'status', '--porcelain', '--untracked-files=normal'], source):
        raise InstallerError('checkout must be clean')
    for name in ('Cargo.lock', 'bun.lock', 'Cargo.toml', 'package.json'):
        if not (source / name).is_file() or (source / name).is_symlink():
            raise InstallerError('actual dependency locks and manifests are required')
    return source


# Exact release allowlist: no live configuration, credentials, workbook or journals.
TEMPLATES = {
    'LICENSE': 'LICENSE',
    'scripts/preflight.py': 'scripts/preflight.py',
    'scripts/backup.py': 'scripts/backup.py',
    'templates/service.env.example': '.env.example',
    'templates/qualification.example.json': 'deploy/qualification.example.json',
    'templates/registry.example.json': 'config/workspace.example.json',
    'templates/openclaw.example.json': 'config/openclaw.example.json',
    'templates/secretary-core.service': 'deploy/secretary-core.service',
    'INSTALLATION.md': 'docs/installation.md',
}
ARTIFACTS = set(TEMPLATES) | {'bin/secretary-core', 'plugins/secretary-tools.tgz'}


def fingerprint(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def native_host():
    return {'system': platform.system(), 'machine': platform.machine(),
            'libc': list(platform.libc_ver())}


def plain_path(path):
    path = Path(os.path.abspath(path))
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise InstallerError('symlinked release paths are forbidden')
    return path


def plugin_contract(archive):
    with tarfile.open(archive, 'r:gz') as packed:
        members = packed.getmembers()
        files = [m for m in members if m.isfile()]
        expected = {'package/package.json', 'package/openclaw.plugin.json', 'package/dist/index.js'}
        if (len(files) != 3 or {m.name for m in files} != expected
                or any(not (m.isfile() or m.isdir()) or m.size > 16 * 1024 * 1024 for m in members)
                or any(m.isdir() and m.name.rstrip('/') not in ('package', 'package/dist') for m in members)):
            raise InstallerError('plugin archive contains unexpected runtime files')
        manifest = json.load(packed.extractfile('package/openclaw.plugin.json'))
    tools = manifest.get('contracts', {}).get('tools', [])
    properties = manifest.get('configSchema', {}).get('properties', {})
    if (manifest.get('id') != 'secretary-tools' or len(tools) != 25 or len(set(tools)) != 25
            or not {'sheets_preview_create_tab', 'sheets_create_tab'} <= set(tools)
            or properties.get('approvalTransportQualified', {}).get('default') is not False
            or properties.get('schedulerQualified', {}).get('default') is not False):
        raise InstallerError('plugin contract or inactive defaults differ')
    return tools


def verify_release(release, revision):
    release = plain_path(release)
    if not release.is_dir() or not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise InstallerError('release directory and full reviewed commit are required')
    found = set()
    for path in release.rglob('*'):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise InstallerError('release contains non-regular entries')
        if path.is_file():
            found.add(path.relative_to(release).as_posix())
    if found != ARTIFACTS | {'release.json'}:
        raise InstallerError('release contains missing or unexpected files')
    receipt = json.loads((release / 'release.json').read_text())
    if (receipt.get('format_version') != 2 or receipt.get('source_commit') != revision
            or receipt.get('production_writes') is not False
            or receipt.get('starts_services') is not False or receipt.get('activates_plugins') is not False
            or set(receipt.get('artifact_hashes', {})) != ARTIFACTS):
        raise InstallerError('release receipt does not match the selected inactive revision')
    for name in ARTIFACTS:
        if fingerprint(release / name) != receipt['artifact_hashes'][name]:
            raise InstallerError('release artifact hash mismatch')
    if not (release / 'bin/secretary-core').stat().st_mode & 0o111:
        raise InstallerError('Core binary must be executable')
    plugin_contract(release / 'plugins/secretary-tools.tgz')
    qualification = json.loads((release / 'templates/qualification.example.json').read_text())
    registry = json.loads((release / 'templates/registry.example.json').read_text())
    environment = (release / 'templates/service.env.example').read_text().splitlines()
    if (qualification.get('qualified') is not False
            or any(s.get('create_tabs', False) is not False for s in registry.get('spreadsheets', []))
            or any(d.get('writable', False) is not False for d in registry.get('documents', []))
            or 'SECRETARY_WRITES_ENABLED=false' not in environment or 'SECRETARY_MOCK_MODE=false' not in environment):
        raise InstallerError('release examples must retain disabled qualification and write gates')
    return receipt


def build_environment(home):
    # Build tools get isolated config/home and caches, never inherited Core/Google secrets.
    allowed = ('PATH', 'CARGO_HOME', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'RUSTUP_HOME',
               'BUN_INSTALL_CACHE_DIR', 'TMPDIR', 'LANG', 'LC_ALL', 'SYSTEMROOT')
    env = {key: os.environ[key] for key in allowed if key in os.environ}
    original_home = Path.home()
    env.setdefault('CARGO_HOME', str(original_home / '.cargo'))
    env.setdefault('RUSTUP_HOME', str(original_home / '.rustup'))
    env['HOME'] = str(home)
    env['XDG_CONFIG_HOME'] = str(home / '.config')
    env['CI'] = '1'
    return env


def tool_versions(source, env):
    versions = {name: run([name, '--version'], source, env, name + ' version', 30)
                for name in ('node', 'bun', 'rustc', 'cargo', 'openclaw')}
    match = re.fullmatch(r'v(\d+)\.(\d+)\.(\d+)', versions['node'])
    version = tuple(map(int, match.groups())) if match else (0, 0, 0)
    if not ((24, 22, 1) <= version < (25, 0, 0) or version >= (26, 12, 1)):
        raise InstallerError('Node version is outside the pinned SDK engine range')
    if (versions['bun'] != '1.2.21' or not versions['rustc'].startswith('rustc 1.95.0 ')
            or not versions['cargo'].startswith('cargo 1.95.0 ')
            or not re.search(r'\b2026\.10\.7\b', versions['openclaw'])):
        raise InstallerError('Bun, Rust/Cargo and OpenClaw must match the qualified versions')
    host = run(['rustc', '-vV'], source, env, 'Rust target', 30)
    versions['rust_host'] = next((line[6:] for line in host.splitlines() if line.startswith('host: ')), '')
    if not versions['rust_host']:
        raise InstallerError('Rust host target unavailable')
    return versions


def stage(source, revision, output, offline):
    if os.geteuid() == 0:
        raise InstallerError('build/stage as an unprivileged operator; reserve root for install-files')
    output = plain_path(output)
    if output == source or output.is_relative_to(source):
        raise InstallerError('stage output must be outside the checkout')
    parent = output.parent
    info = parent.stat()
    if info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise InstallerError('stage parent must be private and owned by the invoking user')
    lock = parent / ('.' + output.name + '.lock')
    descriptor = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    temporary = None
    try:
        if output.exists():
            receipt = verify_release(output, revision)
            if receipt.get('build_host') != native_host():
                raise InstallerError('existing release was built for another native host')
            return {'source_commit': revision, 'reused': True}
        temporary = Path(tempfile.mkdtemp(prefix='.' + output.name + '-', dir=parent))
        home = temporary / '.build-home'
        home.mkdir(mode=0o700)
        env = build_environment(home)
        versions = tool_versions(source, env)
        if not offline:
            run(['bun', 'install', '--frozen-lockfile'], source, env, 'frozen JavaScript install')
        sdk = source / 'node_modules/openclaw/package.json'
        if not sdk.is_file() or json.loads(sdk.read_text()).get('version') != '2026.10.7':
            raise InstallerError('actual locked SDK required; offline staging needs a prior frozen install')
        command = ['cargo', 'build', '--release', '--locked', '--target', versions['rust_host'],
                   '--message-format=json', '-p', 'secretary-core']
        if offline:
            command.append('--offline')
        events = run(command, source, env, 'locked native Core build')
        binaries = []
        for line in events.splitlines():
            event = json.loads(line)
            if (event.get('reason') == 'compiler-artifact' and event.get('target', {}).get('name') == 'secretary-core'
                    and 'bin' in event.get('target', {}).get('kind', []) and event.get('executable')):
                binaries.append(Path(event['executable']))
        if len(binaries) != 1 or not binaries[0].is_file() or binaries[0].is_symlink():
            raise InstallerError('Cargo must report exactly one native Core executable')
        (temporary / 'bin').mkdir()
        shutil.copyfile(binaries[0], temporary / 'bin/secretary-core')
        (temporary / 'bin/secretary-core').chmod(0o755)
        run(['bun', 'run', 'build'], source, env, 'actual SDK build/typecheck')
        run(['bun', 'run', 'validate'], source, env, 'actual SDK validation')
        (temporary / 'plugins').mkdir()
        run(['openclaw', 'plugins', 'pack', '--root', str(source / 'plugins/secretary-tools'),
             '--out', str(temporary / 'plugins/secretary-tools.tgz'), '--json'], source, env, 'plugin packing')
        tools = plugin_contract(temporary / 'plugins/secretary-tools.tgz')
        shutil.rmtree(home)
        for target, origin in TEMPLATES.items():
            destination = temporary / target
            destination.parent.mkdir(parents=True, exist_ok=True)
            plain_path(source / origin)
            if not (source / origin).is_file():
                raise InstallerError('release inputs must be regular tracked files')
            shutil.copyfile(source / origin, destination)
            destination.chmod(0o644)
        reviewed_source(source, revision)
        receipt = {'format_version': 2, 'source_commit': revision, 'build_host': native_host(),
                   'tool_versions': versions, 'tool_count': len(tools), 'production_writes': False,
                   'starts_services': False, 'activates_plugins': False,
                   'lock_hashes': {name: fingerprint(source / name) for name in ('Cargo.lock', 'bun.lock')},
                   'artifact_hashes': {name: fingerprint(temporary / name) for name in sorted(ARTIFACTS)}}
        (temporary / 'release.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
        verify_release(temporary, revision)
        if output.exists():
            raise InstallerError('stage destination appeared during build')
        temporary.rename(output)
        temporary = None
        return {'source_commit': revision, 'reused': False}
    finally:
        if temporary is not None:
            shutil.rmtree(temporary)
        os.close(descriptor)
        lock.unlink()


def inactive_service(root, reused):
    # Look beyond /etc: a vendor/runtime unit could already be waiting to restart.
    locations = ('run/systemd/system', 'usr/local/lib/systemd/system',
                 'usr/lib/systemd/system', 'lib/systemd/system')
    for directory in locations:
        candidate = root / directory / 'secretary-core.service'
        if candidate.exists() or candidate.is_symlink():
            raise InstallerError('an existing vendor/runtime unit requires explicit operator reconciliation')
    for directory in ('etc/systemd/system', *locations):
        for suffix in ('wants', 'requires', 'upholds'):
            if any((root / directory).glob('*.' + suffix + '/secretary-core.service')):
                raise InstallerError('existing service enablement links require operator reconciliation')
    if root == Path('/'):
        output = run(['systemctl', 'show', 'secretary-core.service', '-p', 'LoadState',
                      '-p', 'ActiveState', '-p', 'SubState', '-p', 'UnitFileState', '-p', 'FragmentPath'],
                     label='read-only system unit inspection', timeout=30)
        properties = dict(line.split('=', 1) for line in output.splitlines() if '=' in line)
        if (properties.get('ActiveState') != 'inactive' or properties.get('SubState') != 'dead'
                or properties.get('UnitFileState') not in ('', 'disabled', 'static')
                or properties.get('LoadState') not in (('loaded', 'not-found') if reused else ('not-found',))
                or properties.get('FragmentPath') not in (('', '/etc/systemd/system/secretary-core.service') if reused else ('',))):
            raise InstallerError('effective service state is active, enabled, conflicting or unresolved')


def install_files(release, revision, root):
    receipt = verify_release(release, revision)
    release = plain_path(release)
    root = plain_path(root)
    if root == Path('/') and (os.geteuid() != 0 or platform.system() != 'Linux'):
        raise InstallerError('system file installation requires explicit root execution on Linux')
    if not root.is_dir() or root.stat().st_uid != os.geteuid() or root.stat().st_mode & 0o022:
        raise InstallerError('installation root must be existing, owned and not group/other-writable')
    if receipt.get('build_host') != native_host():
        raise InstallerError('release must match the native OS, architecture and libc')
    destination = plain_path(root / 'opt/secretary')
    unit = plain_path(root / 'etc/systemd/system/secretary-core.service')
    if (destination == release or destination.is_relative_to(release)
            or release.is_relative_to(destination)):
        raise InstallerError('release and installation trees must be separate')
    unit_bytes = (release / 'templates/secretary-core.service').read_bytes()
    # Check every existing target before placing files. Never overwrite configuration/state.
    reused = destination.exists()
    if unit.exists() and (not reused or not unit.is_file() or unit.read_bytes() != unit_bytes):
        raise InstallerError('existing service unit differs; preserve it for an explicit upgrade')
    if reused:
        existing = verify_release(destination, revision)
        if existing != receipt:
            raise InstallerError('existing installation differs; automatic upgrades are forbidden')
    inactive_service(root, reused)
    destination.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    lock = destination.parent / '.secretary-install.lock'
    descriptor = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    temporary = None
    unit_temporary = None
    try:
        inactive_service(root, reused)
        if not reused:
            temporary = Path(tempfile.mkdtemp(prefix='.secretary-install-', dir=destination.parent))
            shutil.copytree(release, temporary, dirs_exist_ok=True)
            for path in temporary.rglob('*'):
                path.chmod(0o755 if path.is_dir() or path.relative_to(temporary).as_posix() == 'bin/secretary-core' else 0o644)
            temporary.chmod(0o755)
            verify_release(temporary, revision)
            if destination.exists():
                raise InstallerError('installation destination appeared during staging')
            temporary.rename(destination)
            temporary = None
        unit.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
        if not unit.exists():
            fd, name = tempfile.mkstemp(prefix='.secretary-unit-', dir=unit.parent)
            unit_temporary = Path(name)
            with os.fdopen(fd, 'wb') as stream:
                stream.write(unit_bytes)
                stream.flush()
                os.fsync(stream.fileno())
            unit_temporary.chmod(0o644)
            os.link(unit_temporary, unit)
        plain_path(unit)
        if unit.read_bytes() != unit_bytes:
            raise InstallerError('service template changed during installation')
        return {'source_commit': revision, 'files_installed': True, 'reused': reused,
                'starts_services': False, 'activates_plugins': False, 'production_writes': False}
    finally:
        if temporary is not None:
            shutil.rmtree(temporary)
        if unit_temporary is not None:
            unit_temporary.unlink()
        os.close(descriptor)
        lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('plan', 'stage', 'verify', 'install-files'))
    parser.add_argument('--source', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--revision', required=True, help='full reviewed source commit')
    parser.add_argument('--output', type=Path, help='new release directory outside the checkout')
    parser.add_argument('--release', type=Path, help='trusted staged release to verify or install')
    parser.add_argument('--offline', action='store_true', help='skip JS install; require prior frozen dependencies, use Cargo offline')
    parser.add_argument('--root', type=Path, default=Path('/'), help='target filesystem root; non-/ is for disposable offline staging')
    args = parser.parse_args()
    try:
        if args.command == 'install-files':
            if args.release is None:
                raise InstallerError('--release is required')
            print(json.dumps(install_files(args.release, args.revision, args.root)))
            return 0
        if args.command == 'verify':
            if args.release is None:
                raise InstallerError('--release is required')
            verify_release(args.release, args.revision)
            print(json.dumps({'verified': True, 'source_commit': args.revision}))
            return 0
        source = reviewed_source(args.source, args.revision)
        if args.command == 'stage':
            if args.output is None:
                raise InstallerError('--output is required')
            print(json.dumps(stage(source, args.revision, args.output, args.offline)))
            return 0
        print(json.dumps({'source_commit': args.revision, 'starts_services': False,
                          'activates_plugins': False, 'production_writes': False,
                          'prerequisites': {'node': '>=24.22.1 <25 || >=26.12.1',
                                            'bun': '1.2.21', 'rust': '1.95.0', 'openclaw': '2026.10.7'}}))
        return 0
    except (InstallerError, OSError, ValueError, subprocess.SubprocessError, tarfile.TarError, KeyError, TypeError, AttributeError) as error:
        message = str(error) if isinstance(error, InstallerError) else 'local prerequisites or command failed'
        print('Installer denied: ' + message, file=sys.stderr)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
